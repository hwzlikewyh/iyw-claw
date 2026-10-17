//! 只读取 SDK 的结构化工具输入/结果，不从模型回复或前端计时猜测循环状态。
use super::{text, RuntimeTask, RuntimeTasks};
use serde_json::Value;

const PENDING_TOOL_LIMIT: usize = 512;

pub(super) fn observe(state: &mut RuntimeTasks, message: &Value) -> bool {
    let Some(blocks) = message
        .pointer("/message/content")
        .and_then(Value::as_array)
    else {
        return false;
    };
    if message["type"] == "assistant" {
        capture_tools(state, blocks, message["parent_tool_use_id"].as_str());
        return false;
    }
    let results: Vec<_> = blocks
        .iter()
        .filter(|block| block["type"] == "tool_result")
        .collect();
    // tool_use_result 是单个工具的完整 Output；多结果消息没有明确对应关系时不猜归属。
    if results.len() != 1 || results[0]["is_error"] == true {
        return false;
    }
    let Some(call) =
        text(results[0], &["tool_use_id"]).and_then(|id| state.pending_tools.remove(&id))
    else {
        return false;
    };
    let Some(output) = message
        .get("tool_use_result")
        .filter(|value| value.is_object())
    else {
        return false;
    };
    match call["name"].as_str() {
        Some("CronCreate") => create(state, &call["input"], output),
        Some("CronDelete") => {
            text(output, &["id"]).is_some_and(|id| end(state, &format!("loop:{id}")))
        }
        Some("CronList") => reconcile(state, output),
        Some("ScheduleWakeup") => wakeup(state, &call, output),
        Some("Agent" | "Task") => agent_result(state, &call, output),
        Some("Monitor" | "Bash" | "PowerShell" | "TaskStop") => {
            super::background_tools::observe(state, &call, output)
        }
        _ => false,
    }
}

fn agent_result(state: &mut RuntimeTasks, call: &Value, output: &Value) -> bool {
    let (Some(agent), Some(tool)) = (text(output, &["agentId"]), text(call, &["id"])) else {
        return false;
    };
    let mut task = state
        .tasks
        .values()
        .find(|task| task.tool_call_id.as_deref() == Some(tool.as_str()))
        .cloned()
        .unwrap_or_else(|| RuntimeTask {
            id: format!("task:{agent}"),
            native_id: agent.clone(),
            ..Default::default()
        });
    task.kind = "subagent".into();
    task.agent_type = Some(crate::models::AgentType::ClaudeCode);
    task.transcript_id = Some(agent);
    task.tool_call_id = Some(tool.clone());
    task.parent_id = state.parent_task_for(call).or(task.parent_id);
    task.objective = text(output, &["prompt"])
        .or_else(|| text(&call["input"], &["prompt"]))
        .or(task.objective);
    task.title = text(output, &["description"])
        .or_else(|| text(&call["input"], &["description"]))
        .unwrap_or_else(|| task.native_id.clone());
    task.model = text(output, &["resolvedModel"]).or(task.model);
    task.goal_scope = state.tool_goals.get(&tool).cloned().or(task.goal_scope);
    task.output_file = text(output, &["outputFile"]).or(task.output_file);
    apply_agent_result(&mut task, output);
    // async_launched 只代表已启动，不能被工具返回误标为完成。
    task.actions.clear();
    state.upsert(task)
}

fn create(state: &mut RuntimeTasks, input: &Value, output: &Value) -> bool {
    let Some(id) = text(output, &["id"]) else {
        return false;
    };
    state.upsert(RuntimeTask {
        id: format!("loop:{id}"),
        native_id: id,
        kind: "loop".into(),
        title: text(input, &["prompt"]).unwrap_or_else(|| "循环任务".into()),
        objective: text(input, &["prompt"]),
        status: "waiting".into(),
        schedule: text(input, &["cron"]),
        summary: text(output, &["humanSchedule"]),
        ..Default::default()
    })
}

fn end(state: &mut RuntimeTasks, id: &str) -> bool {
    let Some(mut task) = state.tasks.get(id).cloned() else {
        return false;
    };
    task.status = "cancelled".into();
    task.next_run = None;
    task.actions.clear();
    state.upsert(task)
}

fn reconcile(state: &mut RuntimeTasks, output: &Value) -> bool {
    let Some(jobs) = output["jobs"].as_array() else {
        return false;
    };
    let ended: Vec<_> = state
        .tasks
        .values()
        .filter(|task| {
            task.kind == "loop"
                && task.native_id != "dynamic"
                && !jobs
                    .iter()
                    .any(|job| job["id"].as_str() == Some(task.native_id.as_str()))
        })
        .map(|task| task.id.clone())
        .collect();
    let mut changed = false;
    for id in ended {
        changed |= end(state, &id);
    }
    for job in jobs {
        changed |= create(state, job, job);
    }
    changed
}

fn wakeup(state: &mut RuntimeTasks, call: &Value, output: &Value) -> bool {
    if output["stopped"] == true {
        return end(state, "loop:dynamic");
    }
    let Some(at) = output["scheduledFor"]
        .as_i64()
        .and_then(chrono::DateTime::from_timestamp_millis)
    else {
        return false;
    };
    state.upsert(RuntimeTask {
        id: "loop:dynamic".into(),
        native_id: "dynamic".into(),
        kind: "loop".into(),
        title: text(&call["input"], &["prompt"]).unwrap_or_else(|| "循环任务".into()),
        objective: text(&call["input"], &["prompt"]),
        status: "waiting".into(),
        next_run: Some(at.to_rfc3339()),
        tool_call_id: text(call, &["id"]),
        summary: text(&call["input"], &["reason"]),
        ..Default::default()
    })
}

fn apply_agent_result(task: &mut RuntimeTask, output: &Value) {
    if output["status"] == "completed" {
        task.status = "completed".into();
        task.output = output["content"].as_array().map(|blocks| {
            super::preview(
                &blocks
                    .iter()
                    .filter_map(|block| block["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        });
    } else if !super::is_terminal(&task.status) {
        task.status = if output["status"] == "async_launched" {
            "running"
        } else {
            "unknown"
        }
        .into();
    }
}

fn capture_tools(state: &mut RuntimeTasks, blocks: &[Value], parent: Option<&str>) {
    for block in blocks.iter().filter(|block| {
        block["type"] == "tool_use"
            && matches!(
                block["name"].as_str(),
                Some(
                    "CronCreate" | "CronDelete" | "CronList" | "ScheduleWakeup" | "Agent" | "Task"
                    | "Monitor" | "Bash" | "PowerShell" | "TaskStop"
                )
            )
    }) {
        let Some(id) = text(block, &["id"]) else {
            continue;
        };
        if state.pending_tools.len() >= PENDING_TOOL_LIMIT {
            state.pending_tools.pop_first();
        }
        let mut tracked = block.clone();
        tracked["_iywParentToolUseId"] = serde_json::json!(parent);
        state.pending_tools.insert(id, tracked);
    }
}
