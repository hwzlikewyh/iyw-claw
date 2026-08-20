use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tokio::sync::{Mutex, OnceCell};
use uuid::Uuid;

use crate::app_error::AppCommandError;
use crate::chat_channel::types::ChannelType;

use super::types::{ProviderSession, ProviderStart, QrPollResponse, QrStartResponse, QrStatus};

const SESSION_GRACE: Duration = Duration::from_secs(10 * 60);
const DEFAULT_RETRY_MS: u64 = 3000;
const MAX_ACTIVE_SESSIONS: usize = 128;
const LIFECYCLE_ACTIVE: u8 = 0;
const LIFECYCLE_COMMITTING: u8 = 1;
const LIFECYCLE_CANCELLED: u8 = 2;

pub(super) struct Session {
    pub id: String,
    pub channel_id: i32,
    pub channel_type: ChannelType,
    pub provider: ProviderSession,
    lifecycle: AtomicU8,
    pub poll_lock: Mutex<()>,
    poll_failures: AtomicU8,
    expires_at: DateTime<Utc>,
    deadline: Instant,
    purge_at: Instant,
    retry_after_ms: u64,
    state: Mutex<SessionState>,
}

#[derive(Clone)]
struct SessionState {
    status: QrStatus,
    error_code: Option<String>,
}

type SessionMap = Mutex<HashMap<String, Arc<Session>>>;

impl Session {
    pub(super) fn is_cancelled(&self) -> bool {
        self.lifecycle.load(Ordering::Acquire) == LIFECYCLE_CANCELLED
    }

    pub(super) fn try_cancel(&self) -> bool {
        self.lifecycle
            .compare_exchange(
                LIFECYCLE_ACTIVE,
                LIFECYCLE_CANCELLED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub(super) fn try_begin_commit(&self) -> bool {
        self.lifecycle
            .compare_exchange(
                LIFECYCLE_ACTIVE,
                LIFECYCLE_COMMITTING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }
}

async fn sessions() -> &'static SessionMap {
    static STORE: OnceCell<SessionMap> = OnceCell::const_new();
    STORE
        .get_or_init(|| async { Mutex::new(HashMap::new()) })
        .await
}

pub(super) async fn prepare(channel_id: i32) {
    prune().await;
    cancel_channel_sessions(channel_id).await;
}

pub(super) async fn insert(
    channel_id: i32,
    channel_type: ChannelType,
    provider: ProviderStart,
) -> Result<QrStartResponse, AppCommandError> {
    let now = Utc::now();
    let expires_in = Duration::from_secs(provider.expires_in_secs);
    let retry_after_ms = match provider.retry_after_ms {
        0 => DEFAULT_RETRY_MS,
        value => value,
    };
    let active = Arc::new(Session {
        id: Uuid::new_v4().to_string(),
        channel_id,
        channel_type,
        provider: provider.session,
        expires_at: now + chrono::Duration::seconds(provider.expires_in_secs as i64),
        deadline: Instant::now() + expires_in,
        purge_at: Instant::now() + expires_in + SESSION_GRACE,
        retry_after_ms,
        lifecycle: AtomicU8::new(LIFECYCLE_ACTIVE),
        poll_lock: Mutex::new(()),
        poll_failures: AtomicU8::new(0),
        state: Mutex::new(SessionState {
            status: QrStatus::Waiting,
            error_code: None,
        }),
    });
    insert_active(active, provider.qr_content).await
}

async fn insert_active(
    active: Arc<Session>,
    qr_content: String,
) -> Result<QrStartResponse, AppCommandError> {
    let mut store = sessions().await.lock().await;
    if store.len() >= MAX_ACTIVE_SESSIONS {
        drop(store);
        super::providers::finish(&active.provider).await;
        return Err(AppCommandError::already_exists("扫码会话过多，请稍后重试"));
    }
    let response = QrStartResponse {
        session_id: active.id.clone(),
        channel_id: active.channel_id,
        channel_type: active.channel_type,
        qr_content,
        expires_at: active.expires_at,
        status: QrStatus::Waiting,
        retry_after_ms: active.retry_after_ms,
    };
    store.insert(active.id.clone(), active);
    Ok(response)
}

pub(super) async fn find(session_id: &str) -> Result<Arc<Session>, AppCommandError> {
    if session_id.trim().is_empty() {
        return Err(AppCommandError::invalid_input("扫码会话 ID 不能为空"));
    }
    prune().await;
    sessions()
        .await
        .lock()
        .await
        .get(session_id)
        .cloned()
        .ok_or_else(|| AppCommandError::not_found("扫码会话已失效，请重新生成二维码"))
}

pub(super) async fn preflight(active: &Session) -> Option<QrPollResponse> {
    let current = active.state.lock().await.clone();
    if current.status.is_terminal() {
        return Some(response_for(active, current));
    }
    if active.is_cancelled() {
        return Some(set_terminal(active, QrStatus::Cancelled, None).await);
    }
    if Instant::now() >= active.deadline {
        return Some(set_terminal(active, QrStatus::Expired, Some("expired")).await);
    }
    None
}

pub(super) async fn keep_waiting(active: &Session) -> QrPollResponse {
    let current = active.state.lock().await.status;
    let status = if current == QrStatus::Scanned {
        QrStatus::Scanned
    } else {
        QrStatus::Waiting
    };
    set_status(active, status, None).await
}

pub(super) fn reset_poll_failures(active: &Session) {
    active.poll_failures.store(0, Ordering::Release);
}

pub(super) fn record_poll_failure(active: &Session) -> u8 {
    active
        .poll_failures
        .fetch_add(1, Ordering::AcqRel)
        .saturating_add(1)
}

pub(super) async fn set_status(
    active: &Session,
    status: QrStatus,
    error_code: Option<&str>,
) -> QrPollResponse {
    let mut state = active.state.lock().await;
    if !state.status.is_terminal() {
        state.status = status;
        state.error_code = error_code.map(str::to_string);
    }
    response_for(active, state.clone())
}

pub(super) async fn set_terminal(
    active: &Session,
    status: QrStatus,
    error_code: Option<&str>,
) -> QrPollResponse {
    let snapshot = {
        let mut state = active.state.lock().await;
        if !state.status.is_terminal() {
            state.status = status;
            state.error_code = error_code.map(str::to_string);
        }
        state.clone()
    };
    super::providers::finish(&active.provider).await;
    sessions().await.lock().await.remove(&active.id);
    response_for(active, snapshot)
}

pub(super) async fn cancel(session_id: &str) -> Result<QrPollResponse, AppCommandError> {
    let active = find(session_id).await?;
    let cancelled = active.try_cancel();
    let _poll_guard = active.poll_lock.lock().await;
    if cancelled || active.is_cancelled() {
        return Ok(set_terminal(&active, QrStatus::Cancelled, None).await);
    }
    Ok(snapshot(&active).await)
}

async fn cancel_channel_sessions(channel_id: i32) {
    let active = remove_where(|session| session.channel_id == channel_id).await;
    for session in active {
        if session.try_cancel() || session.is_cancelled() {
            set_cancelled_state(&session).await;
        }
        super::providers::finish(&session.provider).await;
    }
}

async fn snapshot(active: &Session) -> QrPollResponse {
    let state = active.state.lock().await.clone();
    response_for(active, state)
}

async fn set_cancelled_state(active: &Session) {
    let mut state = active.state.lock().await;
    if !state.status.is_terminal() {
        state.status = QrStatus::Cancelled;
        state.error_code = None;
    }
}

async fn prune() {
    let now = Instant::now();
    let removed = remove_where(|session| now >= session.purge_at).await;
    for session in removed {
        super::providers::finish(&session.provider).await;
    }
}

async fn remove_where(predicate: impl Fn(&Session) -> bool) -> Vec<Arc<Session>> {
    let mut store = sessions().await.lock().await;
    let ids = store
        .iter()
        .filter(|(_, session)| predicate(session))
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    ids.into_iter().filter_map(|id| store.remove(&id)).collect()
}

fn response_for(active: &Session, state: SessionState) -> QrPollResponse {
    QrPollResponse {
        session_id: active.id.clone(),
        channel_id: active.channel_id,
        status: state.status,
        retry_after_ms: active.retry_after_ms,
        error_code: state.error_code,
    }
}
