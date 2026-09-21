use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;

use super::manager::ChatChannelManager;
use super::reconcile;
use super::types::ChannelConnectionStatus;
use crate::db::entities::chat_channel;
use crate::db::service::chat_channel_service;

const CHECK_INTERVAL: Duration = Duration::from_secs(30);
const ERROR_GRACE: chrono::Duration = chrono::Duration::seconds(90);
const WAKE_GAP: chrono::Duration = chrono::Duration::seconds(90);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5 * 60);
const TERMINAL_RETRY_DELAY: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
struct RetryState {
    failures: u32,
    next_attempt: Option<Instant>,
}

pub fn spawn(manager: ChatChannelManager, db: DatabaseConnection) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CHECK_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut previous_tick = Utc::now();
        let mut retries = HashMap::<i32, RetryState>::new();
        interval.tick().await;
        loop {
            interval.tick().await;
            let now = Utc::now();
            let resumed = now.signed_duration_since(previous_tick) > WAKE_GAP;
            previous_tick = now;
            recover_channels(&manager, &db, now, resumed, &mut retries).await;
        }
    })
}

async fn recover_channels(
    manager: &ChatChannelManager,
    db: &DatabaseConnection,
    now: DateTime<Utc>,
    resumed: bool,
    retries: &mut HashMap<i32, RetryState>,
) {
    let channels = match chat_channel_service::list_enabled(db).await {
        Ok(channels) => channels,
        Err(error) => {
            tracing::warn!(error = %error, "[ChatChannel] desktop recovery scan failed");
            return;
        }
    };
    let enabled: HashSet<i32> = channels.iter().map(|channel| channel.id).collect();
    retries.retain(|channel_id, _| enabled.contains(channel_id));
    for channel in channels {
        if should_recover(manager, &channel, now, resumed, retries).await {
            recover_one(manager, db, &channel, retries).await;
        }
    }
}

async fn should_recover(
    manager: &ChatChannelManager,
    channel: &chat_channel::Model,
    now: DateTime<Utc>,
    resumed: bool,
    retries: &HashMap<i32, RetryState>,
) -> bool {
    if retries
        .get(&channel.id)
        .and_then(|state| state.next_attempt)
        .is_some_and(|next| next > Instant::now())
    {
        return false;
    }
    let status = manager.connection_status(channel.id).await;
    if resumed && channel.channel_type != "wecom_agent" {
        return true;
    }
    match status {
        None | Some(ChannelConnectionStatus::Disconnected) => true,
        Some(ChannelConnectionStatus::Error) => channel
            .last_error_at
            .is_none_or(|failed_at| now.signed_duration_since(failed_at) >= ERROR_GRACE),
        Some(ChannelConnectionStatus::Connected | ChannelConnectionStatus::Connecting) => false,
    }
}

async fn recover_one(
    manager: &ChatChannelManager,
    db: &DatabaseConnection,
    channel: &chat_channel::Model,
    retries: &mut HashMap<i32, RetryState>,
) {
    tracing::info!(
        channel_id = channel.id,
        channel_type = channel.channel_type,
        "[ChatChannel] desktop recovery started"
    );
    let outcome =
        reconcile::reconcile_channel(db, manager, channel.id, true, "desktop_recovery").await;
    let error = match outcome {
        Ok(outcome) if outcome.connected => {
            retries.remove(&channel.id);
            return;
        }
        Ok(outcome) => outcome
            .error
            .unwrap_or_else(|| "transport not connected".to_string()),
        Err(error) => error.to_string(),
    };
    schedule_retry(channel.id, &error, retries);
}

fn schedule_retry(channel_id: i32, error: &str, retries: &mut HashMap<i32, RetryState>) {
    let state = retries.entry(channel_id).or_default();
    state.failures = state.failures.saturating_add(1);
    let terminal = error.starts_with("authentication failed")
        || error.starts_with("configuration invalid")
        || error.contains("重新扫码");
    let delay = if terminal {
        TERMINAL_RETRY_DELAY
    } else {
        let shift = state.failures.saturating_sub(1).min(4);
        CHECK_INTERVAL
            .saturating_mul(1_u32 << shift)
            .min(MAX_RETRY_DELAY)
            + Duration::from_secs((channel_id.unsigned_abs() % 7) as u64)
    };
    state.next_attempt = Some(Instant::now() + delay);
    tracing::warn!(
        channel_id,
        retry_after_secs = delay.as_secs(),
        error,
        "[ChatChannel] desktop recovery deferred"
    );
}
