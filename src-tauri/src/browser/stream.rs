use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::command_runner::AgentBrowserCli;
use super::error::{BrowserError, BrowserErrorCode};
use super::frame_protocol::ensure_frame_generations;
use super::stream_lifecycle::{disconnected, spawn_stream_task, stop_entries, stop_entry};
use super::stream_task::StreamTaskContext;
use super::types::{
    BrowserFrameSubscriptionSnapshot, BrowserFrameSubscriptionStatus, BrowserGenerations,
};

const MAX_STREAMS_PER_TAB: usize = 2;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
pub(super) struct BrowserStreamRegistry {
    inner: tokio::sync::Mutex<HashMap<String, StreamSubscription>>,
}

#[derive(Debug)]
pub(super) struct StreamSubscription {
    id: String,
    tab_id: String,
    claim_id: Option<String>,
    generations: BrowserGenerations,
    control: mpsc::Sender<StreamControl>,
    status: Arc<RwLock<BrowserFrameSubscriptionStatus>>,
    pub(super) cancellation: CancellationToken,
    pub(super) task: JoinHandle<()>,
}

pub(super) enum StreamControl {
    Ack {
        seq: u64,
        response: oneshot::Sender<Result<(), BrowserError>>,
    },
    Input {
        messages: Vec<Value>,
        response: oneshot::Sender<Result<(), BrowserError>>,
    },
}

impl BrowserStreamRegistry {
    pub async fn subscribe(
        &self,
        tab_id: String,
        generations: BrowserGenerations,
        session: String,
        cli: AgentBrowserCli,
        cdp_url: String,
        channel: Channel<InvokeResponseBody>,
        claim_id: Option<String>,
    ) -> Result<BrowserFrameSubscriptionSnapshot, BrowserError> {
        let mut inner = self.inner.lock().await;
        if inner.values().filter(|item| item.tab_id == tab_id).count() >= MAX_STREAMS_PER_TAB {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserViewConflict,
                "The browser tab already has the maximum frame subscriptions",
            ));
        }
        let id = Uuid::new_v4().to_string();
        let status = Arc::new(RwLock::new(BrowserFrameSubscriptionStatus::Connecting));
        let cancellation = CancellationToken::new();
        let (control, receiver) = mpsc::channel(32);
        let task = spawn_stream_task(StreamTaskContext {
            session,
            cli,
            cdp_url,
            generations: generations.clone(),
            channel,
            cancellation: cancellation.clone(),
            control: receiver,
            status: Arc::clone(&status),
        });
        inner.insert(
            id.clone(),
            StreamSubscription {
                id: id.clone(),
                tab_id: tab_id.clone(),
                claim_id,
                generations: generations.clone(),
                control,
                status,
                cancellation,
                task,
            },
        );
        Ok(BrowserFrameSubscriptionSnapshot {
            subscription_id: id,
            browser_tab_id: tab_id,
            generations,
            status: BrowserFrameSubscriptionStatus::Connecting,
        })
    }

    pub async fn acknowledge(
        &self,
        subscription_id: &str,
        generations: &BrowserGenerations,
        seq: u64,
    ) -> Result<(), BrowserError> {
        let control = self.control_for(subscription_id, generations).await?;
        let (response, result) = oneshot::channel();
        control
            .send(StreamControl::Ack { seq, response })
            .await
            .map_err(|_| disconnected())?;
        await_control(result).await
    }

    pub async fn input(
        &self,
        subscription_id: &str,
        generations: &BrowserGenerations,
        messages: Vec<Value>,
    ) -> Result<(), BrowserError> {
        let control = self.control_for(subscription_id, generations).await?;
        let (response, result) = oneshot::channel();
        control
            .send(StreamControl::Input { messages, response })
            .await
            .map_err(|_| disconnected())?;
        await_control(result).await
    }

    pub async fn tab_id(
        &self,
        subscription_id: &str,
        generations: &BrowserGenerations,
    ) -> Result<String, BrowserError> {
        let inner = self.inner.lock().await;
        let item = validate_subscription(&inner, subscription_id, generations)?;
        Ok(item.tab_id.clone())
    }

    pub async fn snapshot(
        &self,
        subscription_id: &str,
        generations: &BrowserGenerations,
    ) -> Result<BrowserFrameSubscriptionSnapshot, BrowserError> {
        let (id, tab_id, generations, status) = {
            let inner = self.inner.lock().await;
            let item = validate_subscription(&inner, subscription_id, generations)?;
            (
                item.id.clone(),
                item.tab_id.clone(),
                item.generations.clone(),
                Arc::clone(&item.status),
            )
        };
        let current_status = *status.read().await;
        Ok(BrowserFrameSubscriptionSnapshot {
            subscription_id: id,
            browser_tab_id: tab_id,
            generations,
            status: current_status,
        })
    }

    pub async fn unsubscribe(
        &self,
        subscription_id: &str,
        generations: &BrowserGenerations,
    ) -> Result<(), BrowserError> {
        let entry = {
            let mut inner = self.inner.lock().await;
            validate_subscription(&inner, subscription_id, generations)?;
            inner
                .remove(subscription_id)
                .expect("validated subscription")
        };
        stop_entry(entry).await;
        Ok(())
    }

    pub async fn close_tab(&self, tab_id: &str) {
        let entries = self.take_matching(|entry| entry.tab_id == tab_id).await;
        stop_entries(entries).await;
    }

    pub async fn close_tab_except(&self, tab_id: &str, subscription_id: &str) {
        let entries = self
            .take_matching(|entry| entry.tab_id == tab_id && entry.id != subscription_id)
            .await;
        stop_entries(entries).await;
    }

    pub async fn close_all(&self) {
        let entries = self.take_matching(|_| true).await;
        stop_entries(entries).await;
    }

    pub async fn close_claim(&self, claim_id: &str) {
        let entries = self
            .take_matching(|entry| entry.claim_id.as_deref() == Some(claim_id))
            .await;
        stop_entries(entries).await;
    }

    pub async fn validate_claim_subscription(
        &self,
        subscription_id: &str,
        claim_id: &str,
        tab_id: &str,
        generations: &BrowserGenerations,
    ) -> Result<(), BrowserError> {
        let inner = self.inner.lock().await;
        let entry = validate_subscription(&inner, subscription_id, generations)?;
        if entry.claim_id.as_deref() != Some(claim_id) || entry.tab_id != tab_id {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserViewConflict,
                "The frame subscription does not belong to this browser view claim",
            ));
        }
        Ok(())
    }

    pub async fn promote_claim(
        &self,
        subscription_id: &str,
        claim_id: &str,
    ) -> Result<(), BrowserError> {
        let mut inner = self.inner.lock().await;
        let entry = inner.get_mut(subscription_id).ok_or_else(disconnected)?;
        if entry.claim_id.as_deref() != Some(claim_id) {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserViewConflict,
                "The browser view claim subscription changed",
            ));
        }
        entry.claim_id = None;
        Ok(())
    }

    async fn control_for(
        &self,
        subscription_id: &str,
        generations: &BrowserGenerations,
    ) -> Result<mpsc::Sender<StreamControl>, BrowserError> {
        let inner = self.inner.lock().await;
        Ok(validate_subscription(&inner, subscription_id, generations)?
            .control
            .clone())
    }

    async fn take_matching(
        &self,
        predicate: impl Fn(&StreamSubscription) -> bool,
    ) -> Vec<StreamSubscription> {
        let mut inner = self.inner.lock().await;
        let ids: Vec<String> = inner
            .values()
            .filter(|entry| predicate(entry))
            .map(|entry| entry.id.clone())
            .collect();
        ids.into_iter().filter_map(|id| inner.remove(&id)).collect()
    }
}

fn validate_subscription<'a>(
    inner: &'a HashMap<String, StreamSubscription>,
    id: &str,
    generations: &BrowserGenerations,
) -> Result<&'a StreamSubscription, BrowserError> {
    let entry = inner.get(id).ok_or_else(disconnected)?;
    ensure_frame_generations(&entry.generations, generations)?;
    Ok(entry)
}

async fn await_control(
    response: oneshot::Receiver<Result<(), BrowserError>>,
) -> Result<(), BrowserError> {
    tokio::time::timeout(CONTROL_TIMEOUT, response)
        .await
        .map_err(|_| disconnected())?
        .map_err(|_| disconnected())?
}
