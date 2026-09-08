use serde_json::{json, Value};

use crate::UpstreamError;

pub(super) fn request(params: &Value) -> Result<(Value, Value), UpstreamError> {
    let permissions = params
        .get("permissions")
        .cloned()
        .ok_or_else(|| UpstreamError::InvalidRequest("permission request has no profile".into()))?;
    let granted = json!({ "network": permissions.get("network"), "fileSystem": permissions.get("fileSystem") });
    let request = json!({
        "sessionId": params.get("threadId"),
        "toolCall": {
            "toolCallId": params.get("itemId"), "title": "额外沙箱权限", "kind": "other", "status": "pending",
            "rawInput": { "permissions": permissions, "cwd": params.get("cwd"), "environmentId": params.get("environmentId") },
        },
        "options": [
            { "optionId": "allow_turn", "kind": "allow_once", "name": "允许本轮" },
            { "optionId": "allow_turn_review", "kind": "allow_once", "name": "允许本轮并逐条审核命令" },
            { "optionId": "allow_session", "kind": "allow_always", "name": "允许本会话" },
            { "optionId": "reject", "kind": "reject_once", "name": "拒绝" },
        ],
        "_meta": { "title": "额外沙箱权限", "description": params.get("reason") },
    });
    Ok((request, granted))
}

pub(super) fn response(permissions: &Value, response: Value) -> Value {
    let selected = response.pointer("/outcome/outcome").and_then(Value::as_str) == Some("selected");
    let choice = response
        .pointer("/outcome/optionId")
        .and_then(Value::as_str);
    let granted = selected
        && matches!(
            choice,
            Some("allow_turn" | "allow_turn_review" | "allow_session")
        );
    json!({
        "permissions": if granted { permissions.clone() } else { json!({}) },
        "scope": if granted && choice == Some("allow_session") { "session" } else { "turn" },
        "strictAutoReview": granted && choice == Some("allow_turn_review"),
    })
}
