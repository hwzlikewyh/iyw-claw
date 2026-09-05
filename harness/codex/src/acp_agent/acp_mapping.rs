//! Loss-aware conversion between ACP's session surface and App Server JSON.

use sacp::schema::{
    PermissionOption, PermissionOptionKind, RequestPermissionRequest, ToolCallUpdate,
    ToolCallUpdateFields,
};
use serde_json::{json, Value};

use crate::{Capability, CapabilitySet, UpstreamError};

#[derive(Debug, Clone)]
pub(crate) struct Update {
    pub method: &'static str,
    pub params: Value,
}

pub(crate) fn initialize_response(
    params: &Value,
    capabilities: CapabilitySet,
    load_session: bool,
) -> Value {
    let protocol_version = params
        .get("protocolVersion")
        .or_else(|| params.get("protocol_version"))
        .cloned()
        .unwrap_or_else(|| json!(1));
    json!({
        "protocolVersion": protocol_version,
        "agentCapabilities": {
            "loadSession": load_session,
            "promptCapabilities": {
                "image": capabilities.contains(Capability::Images),
                "audio": false,
                "embeddedContext": true
            },
            "mcpCapabilities": { "http": false, "sse": false },
            "sessionCapabilities": {}
        },
        "agentInfo": { "name": "iyw-claw-codex-inprocess", "title": "星河", "version": env!("CARGO_PKG_VERSION") }
    })
}

pub(crate) fn thread_start_request(params: &Value) -> Result<Value, UpstreamError> {
    required_string(params, "cwd")?;
    Ok(json!({ "method": "thread/start", "params": { "sessionStartSource": "startup" } }))
}

pub(crate) fn thread_resume_request(params: &Value) -> Result<Value, UpstreamError> {
    let session_id = required_string(params, "sessionId")?;
    required_string(params, "cwd")?;
    Ok(
        json!({ "method": "thread/resume", "params": { "threadId": session_id, "excludeTurns": true } }),
    )
}

pub(crate) fn goal_request(method: &str, params: &Value) -> Result<Value, UpstreamError> {
    let thread_id = required_string(params, "sessionId")?;
    let params = match method {
        "thread/goal/get" | "thread/goal/clear" => json!({ "threadId": thread_id }),
        "thread/goal/set" => {
            let mut mapped = json!({ "threadId": thread_id });
            if let Some(objective) = params.get("objective").and_then(Value::as_str) {
                if objective.trim().is_empty() {
                    return Err(UpstreamError::InvalidRequest(
                        "goal objective must not be empty".into(),
                    ));
                }
                mapped["objective"] = Value::String(objective.to_string());
            }
            if let Some(status) = params.get("status").and_then(Value::as_str) {
                let status = match status {
                    "active" | "paused" | "blocked" | "complete" => status.to_string(),
                    "usage_limited" => "usageLimited".to_string(),
                    "budget_limited" => "budgetLimited".to_string(),
                    _ => {
                        return Err(UpstreamError::InvalidRequest(
                            "goal status is not supported by Codex".into(),
                        ))
                    }
                };
                mapped["status"] = Value::String(status);
            }
            if let Some(token_budget) = params.get("tokenBudget") {
                if !token_budget.is_null() && !token_budget.is_i64() {
                    return Err(UpstreamError::InvalidRequest(
                        "goal tokenBudget must be an integer".into(),
                    ));
                }
                mapped["tokenBudget"] = token_budget.clone();
            }
            mapped
        }
        _ => {
            return Err(UpstreamError::InvalidRequest(format!(
                "unsupported ACP goal method: {method}"
            )))
        }
    };
    Ok(json!({ "method": method, "params": params }))
}

pub(crate) fn is_permission_method(method: &str) -> bool {
    matches!(
        method,
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
    )
}

pub(crate) fn permission_request(
    method: &str,
    params: &Value,
) -> Result<RequestPermissionRequest, UpstreamError> {
    if !is_permission_method(method) {
        return Err(UpstreamError::InvalidRequest(format!(
            "unsupported Codex permission method: {method}"
        )));
    }
    if method == "item/commandExecution/requestApproval"
        && (params.get("additionalPermissions").is_some()
            || params.get("additional_permissions").is_some())
    {
        return Err(UpstreamError::InvalidRequest(
            "command approval with additional permissions is not mapped".into(),
        ));
    }
    if params.get("proposedExecpolicyAmendment").is_some()
        || params.get("proposed_execpolicy_amendment").is_some()
        || params.get("proposedNetworkPolicyAmendments").is_some()
        || params.get("proposed_network_policy_amendments").is_some()
        || params.get("grantRoot").is_some()
        || params.get("grant_root").is_some()
    {
        return Err(UpstreamError::InvalidRequest(
            "permission approval contains an unmapped policy amendment".into(),
        ));
    }
    let thread_id =
        required_string(params, "threadId").or_else(|_| required_string(params, "thread_id"))?;
    let _turn_id =
        required_string(params, "turnId").or_else(|_| required_string(params, "turn_id"))?;
    let item_id =
        required_string(params, "itemId").or_else(|_| required_string(params, "item_id"))?;
    let title = params
        .get("command")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| params.get("reason").and_then(Value::as_str))
        .unwrap_or_else(|| {
            if method == "item/fileChange/requestApproval" {
                "Codex file change approval"
            } else {
                "Codex command approval"
            }
        });
    let tool_call = ToolCallUpdate::new(
        item_id,
        ToolCallUpdateFields::new()
            .title(title.to_string())
            .raw_input(params.clone()),
    );
    Ok(RequestPermissionRequest::new(
        thread_id,
        tool_call,
        permission_options(params)?,
    ))
}

pub(crate) fn permission_decision(method: &str, response: &Value) -> Result<Value, UpstreamError> {
    if !is_permission_method(method) {
        return Err(UpstreamError::InvalidRequest(format!(
            "unsupported Codex permission method: {method}"
        )));
    }
    let outcome = response
        .get("outcome")
        .and_then(|value| value.get("outcome").or(Some(value)))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            UpstreamError::InvalidResponse("permission response has no outcome".into())
        })?;
    if matches!(outcome, "cancelled" | "canceled") {
        return Ok(json!({ "decision": "cancel" }));
    }
    if outcome != "selected" {
        return Err(UpstreamError::InvalidResponse(
            "permission response outcome is not selected".into(),
        ));
    }
    let selected = response.get("outcome").unwrap_or(response);
    let option_id = selected
        .get("optionId")
        .or_else(|| selected.get("option_id"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            UpstreamError::InvalidResponse("permission response has no option id".into())
        })?;
    let decision = match option_id {
        "allow_once" => "accept",
        "allow_always" => "acceptForSession",
        "reject_once" | "reject_always" => "decline",
        _ => {
            return Err(UpstreamError::InvalidResponse(format!(
                "unknown ACP permission option: {option_id}"
            )))
        }
    };
    Ok(json!({ "decision": decision }))
}

fn permission_options(params: &Value) -> Result<Vec<PermissionOption>, UpstreamError> {
    let Some(decisions) = params
        .get("availableDecisions")
        .or_else(|| params.get("available_decisions"))
        .and_then(Value::as_array)
    else {
        return Ok(default_permission_options());
    };
    if decisions.iter().any(|decision| {
        !matches!(
            decision.as_str(),
            Some("accept")
                | Some("acceptForSession")
                | Some("accept_for_session")
                | Some("decline")
        )
    }) {
        return Err(UpstreamError::InvalidRequest(
            "permission approval offers an unmapped decision".into(),
        ));
    }
    let mut options = decisions
        .iter()
        .filter_map(|decision| {
            let decision = decision.as_str()?;
            let (id, kind, name) = match decision {
                "accept" => ("allow_once", PermissionOptionKind::AllowOnce, "Allow once"),
                "acceptForSession" | "accept_for_session" => (
                    "allow_always",
                    PermissionOptionKind::AllowAlways,
                    "Allow for session",
                ),
                "decline" => ("reject_once", PermissionOptionKind::RejectOnce, "Reject"),
                _ => return None,
            };
            Some(PermissionOption::new(id, name, kind))
        })
        .collect::<Vec<_>>();
    if options.is_empty() {
        options = default_permission_options();
    }
    Ok(options)
}

fn default_permission_options() -> Vec<PermissionOption> {
    vec![
        PermissionOption::new("allow_once", "Allow once", PermissionOptionKind::AllowOnce),
        PermissionOption::new(
            "allow_always",
            "Allow for session",
            PermissionOptionKind::AllowAlways,
        ),
        PermissionOption::new("reject_once", "Reject", PermissionOptionKind::RejectOnce),
    ]
}

pub(crate) fn prompt_response(params: &Value) -> Value {
    let stop_reason = params
        .pointer("/turn/status")
        .and_then(Value::as_str)
        .map(stop_reason)
        .unwrap_or("end_turn");
    json!({ "stopReason": stop_reason })
}

pub(crate) fn notification_to_update(method: &str, params: &Value) -> Option<Update> {
    match method {
        "item/agentMessage/delta" => Some(Update {
            method: "agent_message_chunk",
            params: json!({
                "content": { "type": "text", "text": params.get("delta")?.as_str()? }
            }),
        }),
        "item/reasoning/summaryTextDelta"
        | "item/reasoning/textDelta"
        | "item/reasoningSummaryText/delta"
        | "item/reasoningContent/delta" => Some(Update {
            method: "agent_thought_chunk",
            params: json!({ "content": { "type": "text", "text": params.get("delta")?.as_str()? } }),
        }),
        "thread/name/updated" => Some(Update {
            method: "session_info_update",
            params: json!({ "title": params.get("threadName").cloned().unwrap_or(Value::Null) }),
        }),
        "thread/goal/updated" => Some(Update {
            method: "session_info_update",
            params: json!({ "_meta": { "codex": { "goal": params.get("goal")? } } }),
        }),
        "thread/goal/cleared" => Some(Update {
            method: "session_info_update",
            params: json!({ "_meta": { "codex": { "goal": Value::Null } } }),
        }),
        "turn/plan/updated" => Some(Update {
            method: "plan",
            params: json!({ "entries": plan_entries(params.get("plan")?) }),
        }),
        "thread/tokenUsage/updated" => Some(Update {
            method: "usage_update",
            params: usage_update(params),
        }),
        _ => None,
    }
}

pub(crate) fn update_payload(kind: &str, mut params: Value) -> Value {
    let object = params.as_object_mut();
    if let Some(object) = object {
        object.insert("sessionUpdate".into(), Value::String(kind.to_string()));
    } else {
        params = json!({ "sessionUpdate": kind, "content": { "type": "text", "text": "" } });
    }
    params
}

fn required_string(params: &Value, field: &str) -> Result<String, UpstreamError> {
    params
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| UpstreamError::InvalidRequest(format!("ACP request has no {field}")))
}

fn stop_reason(status: &str) -> &'static str {
    match status {
        "completed" => "end_turn",
        "interrupted" | "aborted" => "cancelled",
        "failed" => "refusal",
        _ => "end_turn",
    }
}

fn usage_update(params: &Value) -> Value {
    let usage = params
        .get("tokenUsage")
        .cloned()
        .unwrap_or_else(|| json!({}));
    // ACP's `used` is the tokens currently in the active context.  Codex's
    // `total` breakdown is cumulative across the whole session, so using its
    // `totalTokens` makes the displayed context grow past the model window
    // after several turns.  `last` is the latest response/context snapshot
    // (and is also what Codex uses for its own context-window percentage).
    let used = usage
        .get("last")
        .and_then(|last| last.get("totalTokens"))
        .or_else(|| {
            usage
                .get("total")
                .and_then(|total| total.get("totalTokens"))
        })
        .or_else(|| usage.get("totalTokens"))
        .cloned()
        .unwrap_or(json!(0));
    json!({
        "used": used,
        "size": usage.get("modelContextWindow").cloned().unwrap_or(json!(0))
    })
}

fn plan_entries(plan: &Value) -> Vec<Value> {
    plan.as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let content = entry.get("step")?.as_str()?.to_string();
            let status = entry
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("pending")
                .to_ascii_lowercase();
            Some(json!({
                "content": content,
                "priority": "medium",
                "status": status,
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_advertises_only_authorized_optional_capabilities() {
        let prompt_only = CapabilitySet::empty().with(Capability::Prompt);
        let response = initialize_response(&json!({"protocolVersion": 1}), prompt_only, false);
        assert_eq!(response["agentCapabilities"]["loadSession"], false);
        assert_eq!(
            response["agentCapabilities"]["promptCapabilities"]["image"],
            false
        );

        let with_images = prompt_only.with(Capability::Images);
        let response = initialize_response(&json!({"protocolVersion": 1}), with_images, true);
        assert_eq!(response["agentCapabilities"]["loadSession"], true);
        assert_eq!(
            response["agentCapabilities"]["promptCapabilities"]["image"],
            true
        );
    }

    #[test]
    fn maps_agent_delta_without_losing_text() {
        let update = notification_to_update("item/agentMessage/delta", &json!({"delta":"ok"}))
            .expect("delta maps");
        assert_eq!(update.params["content"]["text"], "ok");
        assert_eq!(
            update_payload(update.method, update.params)["sessionUpdate"],
            "agent_message_chunk"
        );
    }

    #[test]
    fn maps_locked_upstream_reasoning_and_usage_shapes() {
        let reasoning = notification_to_update(
            "item/reasoning/summaryTextDelta",
            &json!({"delta": "thinking"}),
        )
        .expect("reasoning maps");
        assert_eq!(reasoning.params["content"]["text"], "thinking");
        let usage = notification_to_update(
            "thread/tokenUsage/updated",
            &json!({
                "tokenUsage": {
                    "total": {"totalTokens": 42},
                    "modelContextWindow": 100
                }
            }),
        )
        .expect("usage maps");
        assert_eq!(usage.params["used"], 42);
        assert_eq!(usage.params["size"], 100);
    }

    #[test]
    fn maps_context_usage_from_latest_snapshot_not_cumulative_total() {
        let usage = notification_to_update(
            "thread/tokenUsage/updated",
            &json!({
                "tokenUsage": {
                    "total": {"totalTokens": 1_300_000},
                    "last": {"totalTokens": 180_000},
                    "modelContextWindow": 1_000_000
                }
            }),
        )
        .expect("usage maps");

        assert_eq!(usage.params["used"], 180_000);
        assert_eq!(usage.params["size"], 1_000_000);
    }

    #[test]
    fn maps_goal_notifications_to_existing_codex_goal_metadata() {
        let goal = notification_to_update(
            "thread/goal/updated",
            &json!({"goal": {"objective": "ship", "status": "active"}}),
        )
        .expect("goal maps");
        assert_eq!(goal.method, "session_info_update");
        assert_eq!(goal.params["_meta"]["codex"]["goal"]["objective"], "ship");
        let cleared =
            notification_to_update("thread/goal/cleared", &json!({})).expect("goal clear maps");
        assert!(cleared.params["_meta"]["codex"]["goal"].is_null());
    }

    #[test]
    fn maps_goal_requests_without_accepting_invalid_status_or_budget() {
        let request = goal_request(
            "thread/goal/set",
            &json!({
                "sessionId": "thread",
                "objective": "ship",
                "status": "budget_limited",
                "tokenBudget": 100
            }),
        )
        .expect("goal request maps");
        assert_eq!(request["params"]["threadId"], "thread");
        assert_eq!(request["params"]["status"], "budgetLimited");
        assert!(goal_request(
            "thread/goal/set",
            &json!({"sessionId": "thread", "status": "unknown"})
        )
        .is_err());
        assert!(goal_request(
            "thread/goal/set",
            &json!({"sessionId": "thread", "tokenBudget": "100"})
        )
        .is_err());
    }

    #[test]
    fn drops_non_text_deltas_instead_of_emitting_invalid_acp() {
        assert!(notification_to_update("item/agentMessage/delta", &json!({"delta": 7})).is_none());
        assert!(notification_to_update(
            "item/reasoningContent/delta",
            &json!({"delta": {"text": "no"}})
        )
        .is_none());
    }

    #[test]
    fn maps_command_approval_to_existing_acp_permission_shape() {
        let request = permission_request(
            "item/commandExecution/requestApproval",
            &json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "item-1",
                "command": "cargo check",
                "availableDecisions": ["accept", "acceptForSession", "decline"]
            }),
        )
        .expect("command approval maps");
        assert_eq!(request.session_id.0.as_ref(), "thread-1");
        assert_eq!(request.options.len(), 3);
        assert_eq!(request.options[0].option_id.0.as_ref(), "allow_once");
    }

    #[test]
    fn maps_nested_acp_permission_outcomes_without_default_allow() {
        assert_eq!(
            permission_decision(
                "item/fileChange/requestApproval",
                &json!({"outcome": {"outcome": "selected", "optionId": "allow_always"}})
            )
            .expect("allow maps"),
            json!({"decision": "acceptForSession"})
        );
        assert_eq!(
            permission_decision(
                "item/fileChange/requestApproval",
                &json!({"outcome": "cancelled"})
            )
            .expect("cancel maps"),
            json!({"decision": "cancel"})
        );
        assert!(permission_decision(
            "item/fileChange/requestApproval",
            &json!({"outcome": {"outcome": "selected", "optionId": "unknown"}})
        )
        .is_err());
    }

    #[test]
    fn refuses_approval_requests_with_unmapped_permission_escalation() {
        assert!(permission_request(
            "item/commandExecution/requestApproval",
            &json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "item-1",
                "additionalPermissions": {"network": true}
            })
        )
        .is_err());
        assert!(permission_request(
            "item/fileChange/requestApproval",
            &json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "item-1",
                "grantRoot": "C:/outside"
            })
        )
        .is_err());
        assert!(permission_request(
            "item/commandExecution/requestApproval",
            &json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "item-1",
                "availableDecisions": ["acceptWithExecpolicyAmendment"]
            })
        )
        .is_err());
    }
}
