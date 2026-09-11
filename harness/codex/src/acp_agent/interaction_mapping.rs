use serde_json::{json, Map, Value};

use crate::UpstreamError;

pub(super) enum InteractionPlan {
    Approval(super::approval_mapping::ApprovalPlan),
    Input { request: Value, ids: Vec<String> },
    McpForm(Value),
    McpUrl(Value),
    Permissions { request: Value, permissions: Value },
    Reply(Value),
}

impl InteractionPlan {
    pub(super) fn new(
        method: &str,
        params: &Value,
        form_supported: bool,
    ) -> Result<Self, UpstreamError> {
        match method {
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                super::approval_mapping::ApprovalPlan::new(method, params).map(Self::Approval)
            }
            "item/permissions/requestApproval" => {
                super::permission_profile::request(params).map(|(request, permissions)| {
                    Self::Permissions {
                        request,
                        permissions,
                    }
                })
            }
            "currentTime/read" => {
                let seconds = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| invalid("system time precedes Unix epoch"))?
                    .as_secs();
                Ok(Self::Reply(json!({ "currentTimeAt": seconds })))
            }
            "item/tool/requestUserInput" if form_supported => input_request(params),
            "item/tool/requestUserInput" => Ok(Self::Reply(json!({ "answers": {} }))),
            "mcpServer/elicitation/request" => mcp_request(params, form_supported),
            _ => Err(invalid("unsupported interaction method")),
        }
    }

    pub(super) fn wire_request(&self) -> Option<(&'static str, &Value)> {
        match self {
            Self::Approval(plan) => Some(("session/request_permission", plan.request())),
            Self::Input { request, .. } | Self::McpForm(request) => {
                Some(("elicitation/create", request))
            }
            Self::McpUrl(request) | Self::Permissions { request, .. } => {
                Some(("session/request_permission", request))
            }
            Self::Reply(_) => None,
        }
    }

    pub(super) fn response(&self, response: Value) -> Result<Value, UpstreamError> {
        match self {
            Self::Approval(plan) => plan.response(&response),
            Self::Input { ids, .. } => input_response(ids, response),
            Self::McpForm(_) => mcp_response(response),
            Self::Permissions { permissions, .. } => {
                Ok(super::permission_profile::response(permissions, response))
            }
            Self::McpUrl(_) => {
                let selected = response
                    .pointer("/outcome/optionId")
                    .and_then(Value::as_str);
                let action = match selected {
                    Some("allow_once") => "accept",
                    Some("reject_once") => "decline",
                    _ => "cancel",
                };
                Ok(json!({ "action": action, "content": null, "_meta": null }))
            }
            Self::Reply(value) => Ok(value.clone()),
        }
    }
}

fn input_request(params: &Value) -> Result<InteractionPlan, UpstreamError> {
    let questions = params
        .get("questions")
        .and_then(Value::as_array)
        .filter(|questions| !questions.is_empty())
        .ok_or_else(|| invalid("input request has no questions"))?;
    let mut properties = Map::new();
    let mut ids = Vec::new();
    for question in questions {
        let id = required_string(question, "id")?;
        if properties
            .insert(id.to_string(), question_schema(question)?)
            .is_some()
        {
            return Err(invalid("input request contains duplicate question ids"));
        }
        ids.push(id.to_string());
    }
    let request = json!({
        "sessionId": required_string(params, "threadId")?,
        "toolCallId": required_string(params, "itemId")?,
        "mode": "form", "message": "请补充以下信息",
        "_meta": { "codex": { "isBlocking": params.get("isBlocking"), "autoResolutionMs": params.get("autoResolutionMs") } },
        "requestedSchema": { "type": "object", "properties": properties, "required": ids },
    });
    Ok(InteractionPlan::Input { request, ids })
}

fn question_schema(question: &Value) -> Result<Value, UpstreamError> {
    let mut schema = json!({
        "type": "string",
        "minLength": 1,
        "title": question.get("header").and_then(Value::as_str).unwrap_or(""),
        "description": required_string(question, "question")?,
        "_meta": { "codex": {
            "isOther": question.get("isOther").and_then(Value::as_bool).unwrap_or(false),
            "isSecret": question.get("isSecret").and_then(Value::as_bool).unwrap_or(false),
        } },
    });
    if let Some(options) = question
        .get("options")
        .and_then(Value::as_array)
        .filter(|options| !options.is_empty())
    {
        let choices = options.iter().map(|option| {
            let label = required_string(option, "label")?;
            Ok(json!({ "const": label, "title": label, "description": option.get("description") }))
        }).collect::<Result<Vec<Value>, UpstreamError>>()?;
        schema["oneOf"] = json!(choices);
    }
    Ok(schema)
}

fn input_response(ids: &[String], response: Value) -> Result<Value, UpstreamError> {
    let mut answers = Map::new();
    if response.get("action").and_then(Value::as_str) != Some("accept") {
        return Ok(json!({ "answers": answers }));
    }
    let content = response
        .get("content")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("accepted input response has no content"))?;
    for id in ids {
        let Some(value) = content.get(id) else {
            continue;
        };
        let values = match value {
            Value::String(text) => vec![text.clone()],
            Value::Array(values) => values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| invalid("input answer must be text"))
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(invalid("input answer must be text")),
        };
        answers.insert(id.clone(), json!({ "answers": values }));
    }
    Ok(json!({ "answers": answers }))
}

fn mcp_request(params: &Value, form_supported: bool) -> Result<InteractionPlan, UpstreamError> {
    match params.get("mode").and_then(Value::as_str) {
        Some("form") if form_supported => Ok(InteractionPlan::McpForm(json!({
            "sessionId": required_string(params, "threadId")?,
            "mode": "form", "message": required_string(params, "message")?,
            "requestedSchema": params.get("requestedSchema").ok_or_else(|| invalid("MCP form has no schema"))?,
            "_meta": params.get("_meta"),
        }))),
        Some("url") => mcp_url_request(params),
        _ => Ok(InteractionPlan::Reply(
            json!({ "action": "decline", "content": null, "_meta": null }),
        )),
    }
}

fn mcp_url_request(params: &Value) -> Result<InteractionPlan, UpstreamError> {
    let url = required_string(params, "url")?;
    let server = required_string(params, "serverName")?;
    let id = required_string(params, "elicitationId")?;
    Ok(InteractionPlan::McpUrl(json!({
        "sessionId": required_string(params, "threadId")?,
        "toolCall": {
            "toolCallId": format!("mcp-elicitation-{id}"),
            "title": required_string(params, "message")?, "kind": "other", "status": "pending",
            "content": [{ "type": "content", "content": { "type": "resource_link", "uri": url, "name": server } }],
        },
        "options": [
            { "optionId": "allow_once", "name": "已完成授权", "kind": "allow_once" },
            { "optionId": "reject_once", "name": "拒绝", "kind": "reject_once" },
        ],
    })))
}

fn mcp_response(response: Value) -> Result<Value, UpstreamError> {
    let action = response
        .get("action")
        .and_then(Value::as_str)
        .filter(|action| matches!(*action, "accept" | "decline" | "cancel"))
        .ok_or_else(|| invalid("MCP elicitation response has an invalid action"))?;
    Ok(
        json!({ "action": action, "content": response.get("content"), "_meta": response.get("_meta") }),
    )
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, UpstreamError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid("interaction request is missing a required string"))
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidRequest(message.into())
}
