//! 只跟踪工具结果明确返回的后台 ID，不能把整个连接的后台计数归给当前任务。
use std::collections::HashSet;

use crate::acp::types::{AcpEvent, DelegationResultSummary, EventEnvelope};
use crate::chat_channel::task_context::{self, TaskContext};
use serde_json::Value;

const MAX_RUNNING_TASKS: usize = 256;
const MAX_RESULT_BYTES: usize = 1_048_576;

#[derive(Clone, Default)]
pub(crate) struct BackgroundTasks {
    pub running: HashSet<String>,
    pub settled: HashSet<String>,
    pub seen: bool,
    pub failed: bool,
    pub stopped: bool,
    pub detached: bool,
}

pub(crate) fn observe(envelope: &EventEnvelope) {
    match &envelope.payload {
        AcpEvent::ToolCall {
            raw_output: Some(output),
            ..
        }
        | AcpEvent::ToolCallUpdate {
            raw_output: Some(output),
            ..
        } => {
            if output.len() <= MAX_RESULT_BYTES {
                if let Ok(value) = serde_json::from_str::<Value>(output) {
                    observe_output(envelope, &value);
                }
            }
        }
        AcpEvent::BackgroundActivity { settled, .. } => {
            for result in settled {
                settle(&envelope.connection_id, &result.task_id, &result.status);
            }
        }
        AcpEvent::DelegationCompleted {
            parent_tool_use_id,
            result,
            ..
        } => {
            for task in task_context::tasks().into_iter().filter(|task| {
                task.connection_id == envelope.connection_id
                    && task.delegations.contains(parent_tool_use_id)
            }) {
                task_context::update(&task.id, |task| {
                    task.background.failed |= matches!(result, DelegationResultSummary::Err { .. });
                });
            }
        }
        _ => {}
    }
}

fn observe_output(envelope: &EventEnvelope, value: &Value) {
    let value = value.get("toolUseResult").unwrap_or(value);
    let id = if value["status"] == "async_launched" {
        value["agentId"].as_str()
    } else {
        value["backgroundTaskId"].as_str()
    };
    if let Some(id) = id.filter(|id| !id.is_empty()) {
        register(envelope, id);
    }
    if let Some(task) = value.get("task") {
        if let (Some(id), Some(status)) = (task["task_id"].as_str(), task["status"].as_str()) {
            let status = if status == "completed"
                && task["exit_code"].as_i64().is_some_and(|code| code != 0)
            {
                "failed"
            } else {
                status
            };
            settle(&envelope.connection_id, id, status);
        }
    }
}

fn register(envelope: &EventEnvelope, id: &str) {
    let Some(task) = task_context::current(&envelope.connection_id)
        .filter(|task| envelope.seq > task.origin.accepted_event_seq)
    else {
        return;
    };
    task_context::update(&task.id, |task| {
        task.background.seen = true;
        if !task.background.settled.contains(id)
            && task.background.running.len() < MAX_RUNNING_TASKS
        {
            task.background.running.insert(id.to_string());
        }
    });
}

fn settle(connection_id: &str, id: &str, status: &str) {
    if !crate::parsers::claude_background::is_terminal_task_status(status) {
        return;
    }
    for task in task_context::tasks()
        .into_iter()
        .filter(|task| task.connection_id == connection_id && task.background.running.contains(id))
    {
        task_context::update(&task.id, |task| {
            task.background.running.remove(id);
            task.background.settled.insert(id.to_string());
            let stopped = matches!(
                status,
                "canceled" | "cancelled" | "killed" | "stopped" | "interrupted"
            );
            task.background.stopped |= stopped;
            task.background.failed |= status != "completed" && !stopped;
        });
    }
}

pub(super) fn terminal_phase(task: &TaskContext) -> &'static str {
    if task.background.failed {
        "failed"
    } else if task.background.stopped {
        "stopped"
    } else {
        "completed"
    }
}
