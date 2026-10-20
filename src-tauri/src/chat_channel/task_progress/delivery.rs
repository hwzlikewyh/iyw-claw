use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use super::{snapshot, Runtime};
use crate::chat_channel::task_context::{self, TaskContext};
use crate::chat_channel::types::{ChannelMessageTarget, ChannelType};
use crate::db::service::{
    chat_channel_service, chat_channel_target_service, sender_context_service,
};

const SEND_TIMEOUT: Duration = Duration::from_secs(20);
const MERGE_WINDOW: Duration = Duration::from_secs(30);
const SEND_WORKERS: usize = 4;

type Outcome = (&'static str, Option<String>, Option<String>);

pub(super) async fn tick(runtime: &Runtime, task: TaskContext) {
    if let Some(current) = task_context::tracked(&task.id) {
        evaluate(runtime, &current).await;
    }
    task_context::update(&task.id, |task| task.in_flight = false);
}

fn terminal(phase: &str) -> bool {
    matches!(phase, "completed" | "failed" | "stopped" | "superseded")
}

async fn evaluate(runtime: &Runtime, task: &TaskContext) {
    let view = snapshot::view(runtime, task).await;
    let phase = view["phase"].as_str().unwrap_or("unknown");
    super::presence::sync(runtime, task, phase).await;
    if terminal(phase) {
        // 普通回合沿用既有结果交付；后台任务在整体结束后追加一次收尾。
        if task.background.detached && phase != "superseded" {
            if task.delivery_error.is_some() && Instant::now() < task.next_due {
                return;
            }
            if !send(runtime, task, true).await {
                return;
            }
        }
        task_context::update(&task.id, |value| value.ended = Some(Instant::now()));
        return;
    }
    task_context::update(&task.id, |value| {
        value.phase = phase.to_string();
        value.background.detached |= phase == "background";
    });
    if phase == "settling" {
        return;
    }
    let urgent = phase != task.phase && phase == "unknown";
    let milestone = task.summary_at.is_some() && task.summary_at != task.summary_sent_at;
    if !urgent
        && !milestone
        && (!task.periodic
            || matches!(phase, "waiting_input" | "unknown")
            || Instant::now() < task.next_due)
    {
        return;
    }
    if !urgent && task.last_sent.is_some_and(|at| at.elapsed() < MERGE_WINDOW) {
        return;
    }
    send(runtime, task, false).await;
}

async fn send(runtime: &Runtime, task: &TaskContext, is_terminal: bool) -> bool {
    static CAPACITY: OnceLock<Arc<Semaphore>> = OnceLock::new();
    let capacity = CAPACITY.get_or_init(|| Arc::new(Semaphore::new(SEND_WORKERS)));
    let Ok(_permit) = Arc::clone(capacity).acquire_owned().await else {
        return false;
    };
    let Some(current) =
        task_context::tracked(&task.id).filter(|value| value.revision == task.revision)
    else {
        return false;
    };
    let fresh = snapshot::view(runtime, &current).await;
    let phase = fresh["phase"].as_str().unwrap_or("unknown");
    if terminal(phase) != is_terminal || phase == "settling" {
        return false;
    }
    let text = notification(&fresh, &current, is_terminal);
    let target = match delivery_target(runtime, &current, is_terminal).await {
        Ok(target) => target,
        Err(error) => {
            record_paused(runtime, &current, (&text, error)).await;
            return false;
        }
    };
    // 异步目标解析期间可能发生取消、停止汇报或回合结算。
    if task_context::tracked(&task.id).is_none_or(|value| value.revision != task.revision) {
        return false;
    }
    let outcome = deliver(
        &runtime.channels,
        &target,
        (&text, (!is_terminal).then(|| task.id.clone())),
    )
    .await;
    task_context::update(&task.id, |value| {
        value.next_due = Instant::now() + value.interval;
        value.summary_sent_at = current.summary_at;
        value.delivery_error = outcome.1.clone();
        if outcome.0 == "sent" {
            value.last_sent = Some(Instant::now());
        }
    });
    let accepted = matches!(outcome.0, "sent" | "queued" | "unknown");
    record(runtime, task, (&text, outcome)).await;
    accepted
}

fn notification(value: &serde_json::Value, task: &TaskContext, terminal: bool) -> String {
    if !terminal && task.summary_at.is_some() && task.summary_at != task.summary_sent_at {
        if let Some(summary) = task.summary.as_deref() {
            return format!("{}\n{summary}", task.origin.title);
        }
    }
    snapshot::text(value)
}

async fn deliver(
    manager: &super::ChatChannelManager,
    target: &ChannelMessageTarget,
    progress: (&str, Option<String>),
) -> Outcome {
    match tokio::time::timeout(
        SEND_TIMEOUT,
        manager.send_progress_to_target(target, progress),
    )
    .await
    {
        Ok(Ok(id)) => ("sent", None, Some(id.0)),
        Ok(Err(error)) => {
            let status = if matches!(
                error,
                crate::chat_channel::error::ChatChannelError::DeliveryDeferred(_)
            ) {
                "queued"
            } else if matches!(
                error,
                crate::chat_channel::error::ChatChannelError::SendFailed(_)
                    | crate::chat_channel::error::ChatChannelError::ConnectionFailed(_)
                    | crate::chat_channel::error::ChatChannelError::Other(_)
            ) {
                "unknown"
            } else {
                "failed"
            };
            (status, Some(error.category().to_string()), None)
        }
        Err(_) => ("unknown", Some("DELIVERY_UNKNOWN".into()), None),
    }
}

async fn record_paused(runtime: &Runtime, task: &TaskContext, message: (&str, &str)) {
    let (text, error) = message;
    task_context::update(&task.id, |value| {
        value.next_due = Instant::now() + value.interval;
        value.summary_sent_at = task.summary_at;
        value.delivery_error = Some(error.into());
    });
    if task.delivery_error.as_deref() != Some(error) {
        tracing::warn!(task_id = %task.id, channel_id = task.origin.route.channel_id,
            category = error, "[channel-progress] delivery paused");
        record(runtime, task, (text, ("failed", Some(error.into()), None))).await;
    }
}

async fn record(runtime: &Runtime, task: &TaskContext, delivery: (&str, Outcome)) {
    let (text, (status, error, receipt)) = delivery;
    if error.is_some() {
        tracing::warn!(task_id = %task.id, status, category = ?error, "[channel-progress] notification unavailable");
    }
    if let Err(error) = crate::db::service::chat_channel_message_log_service::create_log_for_target(
        &runtime.db,
        task.origin.route.channel_id,
        "outbound",
        "task_progress",
        text,
        status,
        error,
        Some(task.id.clone()),
        receipt,
        Some(task.origin.route.target_id.clone()),
    )
    .await
    {
        tracing::warn!(task_id = %task.id, error = %error, "[channel-progress] delivery log failed");
    }
}

async fn delivery_target(
    runtime: &Runtime,
    task: &TaskContext,
    is_terminal: bool,
) -> Result<ChannelMessageTarget, &'static str> {
    let channel_id = task.origin.route.channel_id;
    let channel = chat_channel_service::get_by_id(&runtime.db, channel_id)
        .await
        .map_err(|_| "CHANNEL_QUERY_FAILED")?
        .ok_or("CHANNEL_NOT_FOUND")?;
    if !channel.enabled {
        return Err("CHANNEL_DISABLED");
    }
    let (_, mut target) =
        chat_channel_target_service::resolve(&runtime.db, channel_id, &task.origin.route.target_id)
            .await
            .map_err(|_| "TARGET_NOT_FOUND")?;
    if !target.matches_thread(&task.origin.route.target) {
        return Err("TARGET_CONTEXT_CHANGED");
    }
    if task.origin.channel_type == ChannelType::Weixin {
        let token = sender_context_service::get_weixin_context_token(
            &runtime.db,
            channel_id,
            &task.origin.route.sender_id,
        )
        .await
        .map_err(|_| "TARGET_CONTEXT_UNAVAILABLE")?
        .filter(|token| !token.is_empty());
        if token.is_none() && !is_terminal {
            return Err("TARGET_CONTEXT_EXPIRED");
        }
        target.provider_payload = token.map(|token| serde_json::json!({"context_token":token}));
    }
    Ok(target)
}
