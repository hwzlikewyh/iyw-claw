pub(crate) mod background;
mod control;
mod delivery;
mod presence;
mod snapshot;
mod state;
pub(crate) use control::initial_interval;

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

use super::manager::ChatChannelManager;
use super::task_context::{self, PreparedOrigin, TaskContext};
use crate::acp::manager::ConnectionManager;
use crate::acp::types::PromptInputBlock;

pub(crate) struct ChannelSubmission {
    pub connection_id: String,
    pub folder_id: i32,
    pub conversation_id: i32,
    pub blocks: Vec<PromptInputBlock>,
    pub origin: PreparedOrigin,
}

pub(crate) struct Runtime {
    pub channels: ChatChannelManager,
    pub connections: ConnectionManager,
    pub db: DatabaseConnection,
}

impl Runtime {
    fn clone_ref(&self) -> Self {
        Self {
            channels: self.channels.clone_ref(),
            connections: self.connections.clone_ref(),
            db: self.db.clone(),
        }
    }
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub(crate) async fn submission(
    db: &DatabaseConnection,
    connection: (&str, i32, i32),
    blocks: Vec<PromptInputBlock>,
) -> Result<ChannelSubmission, crate::acp::error::AcpError> {
    let (connection_id, folder_id, conversation_id) = connection;
    let route = task_context::route(connection_id)
        .ok_or_else(|| crate::acp::error::AcpError::protocol("CHANNEL_CONTEXT_MISSING"))?;
    let channel = crate::db::service::chat_channel_service::get_by_id(db, route.channel_id)
        .await
        .map_err(|e| crate::acp::error::AcpError::protocol(e.to_string()))?
        .ok_or_else(|| crate::acp::error::AcpError::protocol("CHANNEL_NOT_FOUND"))?;
    if !channel.enabled {
        return Err(crate::acp::error::AcpError::protocol("CHANNEL_DISABLED"));
    }
    let channel_type = serde_json::from_value(json!(channel.channel_type))
        .map_err(|_| crate::acp::error::AcpError::protocol("CHANNEL_TYPE_INVALID"))?;
    let text = prompt_text(&blocks);
    let report_seconds = initial_interval(text);
    let title = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("渠道任务")
        .chars()
        .filter(|ch| !ch.is_control())
        .take(60)
        .collect();
    Ok(ChannelSubmission {
        connection_id: connection_id.to_string(),
        folder_id,
        conversation_id,
        blocks,
        origin: PreparedOrigin {
            route,
            channel_type,
            title,
            report_seconds,
            prior_delegations: Default::default(),
            accepted_event_seq: 0,
        },
    })
}

fn prompt_text(blocks: &[PromptInputBlock]) -> &str {
    let text = blocks
        .iter()
        .find_map(|block| match block {
            PromptInputBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("渠道任务");
    text.split_once("\n\n")
        .filter(|_| text.starts_with("# Trusted message-channel context"))
        .map(|(_, prompt)| prompt)
        .unwrap_or(text)
}

pub(crate) fn spawn(runtime: Runtime) {
    if RUNTIME.set(runtime).is_err() {
        return;
    }
    tokio::spawn(async {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            for task in task_context::tasks() {
                let runtime = RUNTIME.get().expect("task runtime initialized").clone_ref();
                if task_context::claim(&task.id, task.revision) {
                    tokio::spawn(async move {
                        delivery::tick(&runtime, task).await;
                    });
                }
            }
        }
    });
    tokio::spawn(async {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            super::streaming::sweep(&RUNTIME.get().expect("task runtime initialized").channels)
                .await;
        }
    });
}

pub(crate) async fn view(task: &TaskContext) -> Value {
    let current = task_context::tracked(&task.id).unwrap_or_else(|| task.clone());
    match RUNTIME.get() {
        Some(runtime) => snapshot::view(runtime, &current).await,
        None => json!({"phase": "unknown", "task_id": task.id}),
    }
}

pub(crate) async fn validate_execution(task: &TaskContext) -> Result<(), String> {
    task_context::validate(&task.connection_id, &task.id)?;
    let runtime = RUNTIME.get().ok_or("CHANNEL_RUNTIME_UNAVAILABLE")?;
    let state = runtime
        .connections
        .get_state(&task.connection_id)
        .await
        .ok_or("CHANNEL_CONTEXT_STALE")?;
    let state = state.read().await;
    if state.turn_generation != task.turn_generation || !state.turn_in_flight {
        return Err("CHANNEL_CONTEXT_STALE".into());
    }
    Ok(())
}

pub(crate) async fn control(
    command: &super::types::IncomingCommand,
    prefix: &str,
) -> Option<super::types::RichMessage> {
    control::handle(command, prefix).await
}

async fn cancel(task: &TaskContext) -> super::types::RichMessage {
    let Some(runtime) = RUNTIME.get() else {
        return super::types::RichMessage::error("任务运行环境尚未就绪。");
    };
    match runtime
        .connections
        .cancel_channel_turn(&runtime.db, task)
        .await
    {
        Ok(()) => super::types::RichMessage::info(
            "已提交取消请求，正在等待任务停止；可发送“查看进度”核对。",
        ),
        Err(error) => {
            tracing::warn!(task_id = %task.id, error = %error, "[channel-progress] cancellation rejected");
            super::types::RichMessage::error(
                "无法取消此任务：会话可能已切换，请查询最新进度后操作。",
            )
        }
    }
}

pub(crate) fn record_summary(connection_id: &str, text: &str) -> Result<(), String> {
    let task = task_context::current(connection_id).ok_or("CHANNEL_CONTEXT_MISSING")?;
    if text.trim().is_empty() || text.chars().count() > 500 || text.chars().any(char::is_control) {
        return Err("INVALID_PROGRESS_SUMMARY".into());
    }
    task_context::update(&task.id, |task| {
        if task.summary.as_deref() != Some(text.trim()) {
            task.summary = Some(text.trim().to_string());
            task.summary_at = Some(Instant::now());
        }
    });
    Ok(())
}

pub(crate) async fn sync_presence(connection_id: &str) {
    let (Some(runtime), Some(task)) = (RUNTIME.get(), task_context::current(connection_id)) else {
        return;
    };
    let view = snapshot::view(runtime, &task).await;
    presence::sync(runtime, &task, view["phase"].as_str().unwrap_or("unknown")).await;
}
