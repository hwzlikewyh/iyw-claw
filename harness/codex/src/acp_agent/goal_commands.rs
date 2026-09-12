use sacp::{Client, ConnectionTo, Responder};
use serde_json::{json, Value};

use crate::{Capability, CapabilitySet, UpstreamClient, UpstreamError};

use super::{acp_mapping, send_update, to_sacp_error, PendingPrompt};

const CONTEXT_START: &str = "<!-- IYW_CLAW_USER_CONTEXT_V1_START -->";
const CONTEXT_END: &str = "<!-- IYW_CLAW_USER_CONTEXT_V1_END -->";

pub(super) struct GoalCommand {
    request: Value,
    starts_run: bool,
}

pub(super) struct PromptContext<'a> {
    pub session_id: &'a Option<String>,
    pub pending: &'a mut Option<PendingPrompt>,
    pub connection: &'a ConnectionTo<Client>,
    pub capabilities: CapabilitySet,
}

pub(super) fn parse(params: &Value) -> Result<Option<GoalCommand>, UpstreamError> {
    let Some(blocks) = params.get("prompt").and_then(Value::as_array) else {
        return Ok(None);
    };
    let mut blocks = blocks.iter();
    let mut first = blocks.next();
    if first.is_some_and(is_host_context) {
        first = blocks.next();
    }
    let Some(text) = first
        .and_then(|block| block.get("text"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let Some(arguments) = text.trim_start().strip_prefix("/goal") else {
        return Ok(None);
    };
    if !arguments.is_empty() && !arguments.starts_with(char::is_whitespace) {
        return Ok(None);
    }
    if blocks.any(|block| {
        block
            .get("text")
            .and_then(Value::as_str)
            .is_none_or(|text| !text.trim().is_empty())
    }) {
        return Err(invalid(
            "Goal commands require a single text objective; send attachments in a separate message",
        ));
    }
    let id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("goal request has no sessionId"))?;
    command(id, arguments.trim()).map(Some)
}

fn is_host_context(block: &Value) -> bool {
    block
        .get("text")
        .and_then(Value::as_str)
        .is_some_and(|text| {
            text.starts_with(CONTEXT_START) && text.trim_end().ends_with(CONTEXT_END)
        })
}

fn command(id: &str, arguments: &str) -> Result<GoalCommand, UpstreamError> {
    let (method, fields, starts_run) = match arguments {
        "" | "status" => ("thread/goal/get", json!({}), false),
        "clear" => ("thread/goal/clear", json!({}), false),
        "pause" => ("thread/goal/set", json!({"status": "paused"}), false),
        "resume" => ("thread/goal/set", json!({"status": "active"}), true),
        "set" | "edit" => return Err(invalid("Goal objective is required")),
        text => {
            let objective = text
                .strip_prefix("set ")
                .or_else(|| text.strip_prefix("edit "))
                .unwrap_or(text)
                .trim();
            if objective.is_empty() {
                return Err(invalid("Goal objective is required"));
            }
            (
                "thread/goal/set",
                json!({"objective": objective, "status": "active"}),
                true,
            )
        }
    };
    let mut params = fields;
    params["sessionId"] = json!(id);
    Ok(GoalCommand {
        request: acp_mapping::goal_request(method, &params)?,
        starts_run,
    })
}

pub(super) async fn handle_prompt(
    upstream: &UpstreamClient,
    context: PromptContext<'_>,
    input: (GoalCommand, Responder<Value>),
) -> Result<(), sacp::Error> {
    let (command, responder) = input;
    let id = command.request["params"]["threadId"]
        .as_str()
        .unwrap_or_default();
    let result = if context.session_id.as_deref() != Some(id) {
        Err(invalid("goal request does not match the bound session"))
    } else if context.pending.is_some() {
        Err(invalid("a Codex prompt is already pending"))
    } else {
        execute(upstream, &command, context.capabilities).await
    };
    match result {
        Err(error) => {
            let _ = responder.respond_with_error(to_sacp_error(error));
        }
        Ok(response) => {
            send_summary(context.connection, context.session_id, &response)?;
            if command.starts_run {
                *context.pending = Some(PendingPrompt {
                    thread_id: id.to_string(),
                    turn_id: String::new(),
                    responder,
                    goal_run: true,
                    last_completed_turn: None,
                });
            } else {
                let _ = responder.respond(json!({"stopReason": "end_turn"}));
            }
        }
    }
    Ok(())
}

pub(super) async fn execute(
    upstream: &UpstreamClient,
    command: &GoalCommand,
    capabilities: CapabilitySet,
) -> Result<Value, UpstreamError> {
    if !capabilities.contains(Capability::Goals) {
        return Err(invalid("Goals are not enabled for this Codex runtime"));
    }
    let id = command.request["params"]["threadId"]
        .as_str()
        .unwrap_or_default();
    upstream
        .request_json_for_thread(id, command.request.clone())
        .await
}

pub(super) async fn steer(
    upstream: &UpstreamClient,
    context: PromptContext<'_>,
    command: GoalCommand,
) -> Result<Value, UpstreamError> {
    let id = command.request["params"]["threadId"]
        .as_str()
        .unwrap_or_default();
    if context.session_id.as_deref() != Some(id) {
        return Err(invalid("goal request does not match the bound session"));
    }
    let Some(pending) = context.pending.as_mut() else {
        return Ok(json!({"outcome": "promptRequired"}));
    };
    let response = execute(upstream, &command, context.capabilities).await?;
    if command.starts_run {
        pending.goal_run = true;
    }
    send_summary(context.connection, context.session_id, &response)
        .map_err(|error| UpstreamError::Io(error.to_string()))?;
    Ok(json!({"outcome": "injected"}))
}

pub(super) fn send_summary(
    cx: &ConnectionTo<Client>,
    session_id: &Option<String>,
    response: &Value,
) -> Result<(), sacp::Error> {
    let text = match response.get("goal").filter(|goal| !goal.is_null()) {
        Some(goal) => format!(
            "Goal ({}): {}",
            goal["status"].as_str().unwrap_or("unknown"),
            goal["objective"].as_str().unwrap_or_default()
        ),
        None if response.get("cleared").and_then(Value::as_bool) == Some(true) => {
            "Goal cleared.".to_string()
        }
        None => "No goal is currently set.".to_string(),
    };
    send_update(
        cx,
        session_id,
        "agent_message_chunk",
        json!({"content": {"type": "text", "text": text}}),
    )
}

pub(super) async fn pause(upstream: &UpstreamClient, id: &str) -> Result<(), UpstreamError> {
    if !is_active(upstream, id).await? {
        return Ok(());
    }
    let request = acp_mapping::goal_request(
        "thread/goal/set",
        &json!({"sessionId": id, "status": "paused"}),
    )?;
    upstream
        .request_json_for_thread(id, request)
        .await
        .map(|_| ())
}

pub(super) async fn prepare_cancel(
    upstream: &UpstreamClient,
    id: &str,
) -> Result<(), UpstreamError> {
    pause(upstream, id).await?;
    if upstream.active_turn_for(id).await.is_some() {
        return Ok(());
    }
    // 取消可能先于自动回合的 started 通知；从绑定线程核对实际运行回合。
    let response = upstream
        .request_json_for_thread(
            id,
            json!({
                "method": "thread/read", "params": {"threadId": id, "includeTurns": true}
            }),
        )
        .await?;
    let turn = response
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .and_then(|turns| {
            turns
                .iter()
                .rev()
                .find(|turn| turn["status"] == "inProgress")
        });
    if let Some(turn_id) = turn.and_then(|turn| turn["id"].as_str()) {
        upstream.observe_goal_turn(id, turn_id).await?;
    }
    Ok(())
}

pub(super) async fn is_active(upstream: &UpstreamClient, id: &str) -> Result<bool, UpstreamError> {
    let request = acp_mapping::goal_request("thread/goal/get", &json!({"sessionId": id}))?;
    let response = upstream.request_json_for_thread(id, request).await?;
    Ok(response.pointer("/goal/status").and_then(Value::as_str) == Some("active"))
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidRequest(message.into())
}
