use serde_json::{json, Value};

use crate::UpstreamError;

pub(super) struct ApprovalPlan {
    request: Value,
    choices: Vec<(String, Value)>,
}

impl ApprovalPlan {
    pub(super) fn new(method: &str, params: &Value) -> Result<Self, UpstreamError> {
        validate_request(method, params)?;
        let decisions = params
            .get("availableDecisions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| {
                vec![
                    json!("accept"),
                    json!("acceptForSession"),
                    json!("decline"),
                    json!("cancel"),
                ]
            });
        if decisions.is_empty() {
            return Err(invalid("approval has no available decisions"));
        }
        let mut choices = Vec::new();
        let mut options = Vec::new();
        for (index, decision) in decisions.into_iter().enumerate() {
            let (id, kind, name) = decision_option(&decision, index)?;
            if choices.iter().any(|(existing, _)| existing == &id) {
                return Err(invalid("duplicate approval option"));
            }
            choices.push((id.clone(), decision));
            options.push(json!({ "optionId": id, "kind": kind, "name": name }));
        }
        let title = params
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                if method == "item/fileChange/requestApproval" {
                    "修改文件"
                } else {
                    "执行命令"
                }
            });
        let request = json!({
            "sessionId": params["threadId"],
            "toolCall": { "toolCallId": params["itemId"], "title": title, "status": "pending",
                "kind": if method == "item/fileChange/requestApproval" { "edit" } else { "execute" },
                "rawInput": params, "content": permission_details(params),
            },
            "options": options,
            "_meta": { "permission": { "version": 1, "title": title, "description": params.get("reason") } },
        });
        Ok(Self { request, choices })
    }

    pub(super) fn request(&self) -> &Value {
        &self.request
    }

    pub(super) fn response(&self, response: &Value) -> Result<Value, UpstreamError> {
        let outcome = response
            .pointer("/outcome/outcome")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("approval response has no outcome"))?;
        if outcome == "cancelled" {
            return Ok(json!({ "decision": "cancel" }));
        }
        if outcome != "selected" {
            return Err(invalid("approval response outcome is invalid"));
        }
        let id = response
            .pointer("/outcome/optionId")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("approval response has no selected option"))?;
        let (_, decision) = self
            .choices
            .iter()
            .find(|(candidate, _)| candidate == id)
            .ok_or_else(|| invalid("approval selected an option that was not offered"))?;
        Ok(json!({ "decision": decision }))
    }
}

fn validate_request(method: &str, params: &Value) -> Result<(), UpstreamError> {
    use codex_app_server_protocol::{
        CommandExecutionRequestApprovalParams, FileChangeRequestApprovalParams,
    };
    let valid = match method {
        "item/commandExecution/requestApproval" => {
            serde_json::from_value::<CommandExecutionRequestApprovalParams>(params.clone()).is_ok()
        }
        "item/fileChange/requestApproval" => {
            serde_json::from_value::<FileChangeRequestApprovalParams>(params.clone()).is_ok()
        }
        _ => false,
    };
    if !valid {
        return Err(invalid(
            "approval request does not match the locked protocol",
        ));
    }
    Ok(())
}

fn decision_option(
    decision: &Value,
    index: usize,
) -> Result<(String, &'static str, String), UpstreamError> {
    use codex_app_server_protocol::CommandExecutionApprovalDecision;
    serde_json::from_value::<CommandExecutionApprovalDecision>(decision.clone())
        .map_err(|_| invalid("approval decision does not match the locked protocol"))?;
    let (id, kind, name) = match decision.as_str() {
        Some("accept") => ("allow_once", "allow_once", "允许一次"),
        Some("acceptForSession") => ("allow_always", "allow_always", "允许本会话"),
        Some("decline") => ("reject_once", "reject_once", "拒绝并继续任务"),
        Some("cancel") => ("cancel", "reject_once", "拒绝并停止本轮"),
        _ => return amendment_option(decision, index),
    };
    Ok((id.into(), kind, name.into()))
}

fn amendment_option(
    decision: &Value,
    index: usize,
) -> Result<(String, &'static str, String), UpstreamError> {
    if let Some(prefix) = decision.pointer("/acceptWithExecpolicyAmendment/execpolicy_amendment") {
        let rendered =
            serde_json::to_string(prefix).map_err(|_| invalid("invalid command rule"))?;
        return Ok((
            format!("rule-{index}"),
            "allow_always",
            format!("允许并记住命令前缀：{rendered}"),
        ));
    }
    let amendment = decision
        .get("applyNetworkPolicyAmendment")
        .and_then(|value| value.get("network_policy_amendment"))
        .ok_or_else(|| invalid("unsupported approval decision"))?;
    let host = amendment["host"]
        .as_str()
        .ok_or_else(|| invalid("network rule has no host"))?;
    let (kind, action) = if amendment["action"] == "allow" {
        ("allow_always", "允许")
    } else {
        ("reject_always", "阻止")
    };
    Ok((
        format!("network-{index}"),
        kind,
        format!("{action}并记住主机：{host}"),
    ))
}

fn permission_details(params: &Value) -> Vec<Value> {
    ["additionalPermissions", "grantRoot", "networkApprovalContext"].into_iter().filter_map(|key| {
        let value = params.get(key).filter(|value| !value.is_null())?;
        Some(json!({ "type": "content", "content": { "type": "text", "text": format!("{key}: {value}") } }))
    }).collect()
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidRequest(message.into())
}
