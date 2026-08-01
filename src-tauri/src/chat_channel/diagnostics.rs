//! Channel diagnostics: quick check (credentials + transport, no agent
//! execution) and a full round-trip loop (controlled probe through the real
//! dispatcher → workspace → Agent → prompt → TurnComplete → outbound).
//!
//! Every diagnostic run gets a `diagnostic_id`; the probe inbound shares that
//! id as its trace id so the message log reconstructs the whole chain.

use std::time::Duration;

use chrono::Utc;
use sea_orm::DatabaseConnection;
use serde::Serialize;

use super::manager::ChatChannelManager;
use super::readiness::{evaluate_readiness, ChannelReadinessReport};
use super::types::{ChannelMessageTarget, IncomingCommand};
use crate::app_error::AppCommandError;
use crate::db::service::{chat_channel_message_log_service, chat_channel_service};

/// Probe text sent to the agent during a full-loop diagnostic. Kept
/// deliberately trivial and explicit so it can never be mistaken for real
/// work.
pub const FULL_LOOP_PROBE: &str = "【iyw-claw 通道自检】请只回复：通道正常。";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelDiagnostic {
    pub diagnostic_id: String,
    pub channel_id: i32,
    /// "quick" | "full"
    pub kind: String,
    pub started_at: String,
    pub finished_at: String,
    pub readiness: ChannelReadinessReport,
    pub roundtrip: Option<RoundtripResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundtripResult {
    pub probe_trace_id: String,
    pub enqueued: bool,
    pub outbound_count: u64,
    pub verified: bool,
    pub details: Vec<String>,
}

/// Quick check: credentials + transport + workspace/agent status. Never
/// spawns an agent or executes user work.
pub async fn quick_check_core(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    channel_id: i32,
) -> Result<ChannelDiagnostic, AppCommandError> {
    run_diagnostic(db, manager, channel_id, "quick").await
}

/// Full loop: quick check plus a controlled probe through the real pipeline.
pub async fn full_loop_core(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    channel_id: i32,
) -> Result<ChannelDiagnostic, AppCommandError> {
    run_diagnostic(db, manager, channel_id, "full").await
}

async fn run_diagnostic(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    channel_id: i32,
    kind: &str,
) -> Result<ChannelDiagnostic, AppCommandError> {
    let diagnostic_id = format!("diag-{}-{}", Utc::now().timestamp_millis(), channel_id);
    let started_at = Utc::now();
    let model = chat_channel_service::get_by_id(db, channel_id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| AppCommandError::not_found(format!("Chat channel {channel_id} not found")))?;

    let readiness = evaluate_readiness(db, manager, &model).await;

    let roundtrip = if kind == "full" {
        Some(run_roundtrip(db, manager, channel_id, &diagnostic_id).await)
    } else {
        None
    };

    Ok(ChannelDiagnostic {
        diagnostic_id,
        channel_id,
        kind: kind.to_string(),
        started_at: started_at.to_rfc3339(),
        finished_at: Utc::now().to_rfc3339(),
        readiness,
        roundtrip,
    })
}

/// Enqueue a synthetic inbound with the diagnostic trace id and wait for an
/// outbound row stamped with the same id.
async fn run_roundtrip(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    channel_id: i32,
    trace_id: &str,
) -> RoundtripResult {
    let command = IncomingCommand {
        channel_id,
        sender_id: "__diagnostic__".to_string(),
        sender_name: Some("通道自检".to_string()),
        command_text: FULL_LOOP_PROBE.to_string(),
        callback_data: None,
        target: ChannelMessageTarget::channel(channel_id),
        metadata: serde_json::json!({ "diagnostic": true }),
        message_trace_id: trace_id.to_string(),
        provider_message_id: Some(format!("{trace_id}:probe")),
        received_at: Utc::now(),
    };

    let enqueued = manager.command_sender().try_send(command).is_ok();
    let mut details = Vec::new();
    if !enqueued {
        details.push("消息队列已满，探针未入队，请稍后重试".to_string());
        return RoundtripResult {
            probe_trace_id: trace_id.to_string(),
            enqueued: false,
            outbound_count: 0,
            verified: false,
            details,
        };
    }
    details.push("探针已入队，等待完整回环".to_string());

    // Poll the message log for outbound rows carrying this trace id.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    let mut outbound_count: u64 = 0;
    let mut verified = false;
    loop {
        let rows = chat_channel_message_log_service::list_by_trace(db, trace_id, 50).await;
        if let Ok(rows) = rows {
            outbound_count = rows.iter().filter(|r| r.direction == "outbound").count() as u64;
            verified = rows
                .iter()
                .any(|r| r.direction == "outbound" && r.status == "sent");
            if verified {
                details.push(format!("已收到 {outbound_count} 条带 trace 的出站消息，回环验证成功"));
                break;
            }
            if outbound_count > 0 {
                details.push("产生出站消息但状态非 sent，请查看渠道日志".to_string());
                break;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            details.push("等待出站回复超时（90s），请检查 Agent 是否可完成一轮回复".to_string());
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    RoundtripResult {
        probe_trace_id: trace_id.to_string(),
        enqueued,
        outbound_count,
        verified,
        details,
    }
}
