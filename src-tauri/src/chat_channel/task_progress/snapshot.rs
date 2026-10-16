use serde_json::{json, Value};

use super::Runtime;
use crate::chat_channel::task_context::TaskContext;

pub(super) async fn view(runtime: &Runtime, task: &TaskContext) -> Value {
    let Some(state) = runtime.connections.get_state(&task.connection_id).await else {
        return json!({"task_id": task.id, "title": task.origin.title, "phase": "unknown",
            "delivery_error": task.delivery_error, "elapsed_seconds": task.started.elapsed().as_secs()});
    };
    let state = state.read().await;
    let (phase, children) = super::state::phase(&state, task);
    let activity = state.activity.snapshot();
    let last_activity = [
        activity.text_at,
        activity.tool_output_at,
        activity.tool_started_at,
    ]
    .into_iter()
    .flatten()
    .max()
    .map(|at| at.to_rfc3339());
    let tools = if state.turn_generation == task.turn_generation {
        state
            .active_tool_calls
            .values()
            .filter(|tool| {
                matches!(
                    tool.status,
                    crate::acp::session_state::ToolCallStatus::Pending
                        | crate::acp::session_state::ToolCallStatus::InProgress
                )
            })
            .count()
    } else {
        0
    };
    json!({
        "task_id": task.id, "title": task.origin.title, "phase": phase,
        "elapsed_seconds": task.started.elapsed().as_secs(), "active_tools": tools,
        "background_tasks": children, "last_activity_at": last_activity,
        "summary": task.summary, "summary_source": "agent_reported",
        "summary_age_seconds": task.summary_at.map(|at| at.elapsed().as_secs()),
        "delivery_error": task.delivery_error,
        "periodic_enabled": task.periodic, "interval_seconds": task.interval.as_secs(),
    })
}

pub(super) fn text(value: &Value) -> String {
    let phase = match value["phase"].as_str().unwrap_or("unknown") {
        "running" if value["active_tools"].as_u64().unwrap_or(0) > 0 => "正在等待工具执行结果",
        "running" => "正在处理，尚无新的已确认结果",
        "background" => "后台任务仍在处理",
        "settling" => "正在结算任务结果",
        "waiting_input" => "等待你的回答或确认",
        "completed" => "任务处理已结束",
        "failed" => "任务或后台子任务报告失败，请查看原会话中的结果",
        "stopped" => "任务已停止",
        _ => "当前运行状态无法确认",
    };
    let id = value["task_id"].as_str().unwrap_or_default();
    let mut text = format!(
        "{} #{} · 已运行 {} 分钟\n{}",
        value["title"].as_str().unwrap_or("任务进度"),
        id.chars().take(8).collect::<String>(),
        value["elapsed_seconds"].as_u64().unwrap_or(0) / 60,
        phase
    );
    if let Some(summary) = value["summary"].as_str() {
        text.push_str(&format!(
            "\n最近阶段说明（Agent 提供，{} 秒前）：{summary}",
            value["summary_age_seconds"].as_u64().unwrap_or(0)
        ));
    }
    text
}
