use serde_json::{json, Value};

use crate::{Capability, CapabilitySet, UpstreamError};

const MAX_RESOURCE_BYTES: usize = 256 * 1024;
const MAX_PROMPT_RESOURCE_BYTES: usize = 2 * 1024 * 1024;
const MAX_URI_BYTES: usize = 4 * 1024;

#[derive(Default)]
struct PromptBudget {
    resource_bytes: usize,
}

pub(super) fn turn_start_request(
    params: &Value,
    capabilities: CapabilitySet,
) -> Result<Value, UpstreamError> {
    let thread_id = required_string(params, "sessionId")?;
    let prompt = params
        .get("prompt")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            UpstreamError::InvalidRequest("session/prompt has no prompt blocks".into())
        })?;
    let mut budget = PromptBudget::default();
    let input = prompt
        .iter()
        .map(|content| content_to_user_input(content, &mut budget, capabilities))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "method": "turn/start",
        "params": { "threadId": thread_id, "input": input }
    }))
}

fn content_to_user_input(
    value: &Value,
    budget: &mut PromptBudget,
    capabilities: CapabilitySet,
) -> Result<Value, UpstreamError> {
    let content = value.get("content").unwrap_or(value);
    let kind = content
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("text");
    match kind {
        "text" => text_input(required_string(content, "text")?),
        "image" if capabilities.contains(Capability::Images) => image_input(content),
        "image" => Err(UpstreamError::InvalidRequest(
            "image input is not enabled for this Codex session".into(),
        )),
        "resource_link" => resource_link_input(content, budget),
        "resource" => embedded_resource_input(content, budget),
        _ => Err(UpstreamError::InvalidRequest(format!(
            "unsupported ACP content block: {kind}"
        ))),
    }
}

fn text_input(text: String) -> Result<Value, UpstreamError> {
    Ok(json!({ "type": "text", "text": text }))
}

fn image_input(content: &Value) -> Result<Value, UpstreamError> {
    let data = required_string(content, "data")?;
    let mime = content
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or("image/png");
    Ok(json!({ "type": "image", "url": format!("data:{mime};base64,{data}") }))
}

fn resource_link_input(content: &Value, budget: &mut PromptBudget) -> Result<Value, UpstreamError> {
    let uri = resource_uri(content)?;
    let name = content
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("resource");
    let description = content
        .get("description")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let text = match description {
        Some(description) => format!("Referenced resource: {name}\nURI: {uri}\n{description}"),
        None => format!("Referenced resource: {name}\nURI: {uri}"),
    };
    reserve_resource(budget, text.len())?;
    text_input(text)
}

fn embedded_resource_input(
    content: &Value,
    budget: &mut PromptBudget,
) -> Result<Value, UpstreamError> {
    let resource = content.get("resource").ok_or_else(|| {
        UpstreamError::InvalidRequest("embedded resource has no resource body".into())
    })?;
    if resource.get("blob").is_some() {
        return Err(UpstreamError::InvalidRequest(
            "binary embedded resources are not supported".into(),
        ));
    }
    let uri = resource_uri(resource)?;
    let text = required_string(resource, "text")?;
    let rendered = format!("Embedded resource ({uri}):\n{text}");
    reserve_resource(budget, rendered.len())?;
    text_input(rendered)
}

fn resource_uri(content: &Value) -> Result<String, UpstreamError> {
    let uri = required_string(content, "uri")?;
    if uri.len() > MAX_URI_BYTES {
        return Err(UpstreamError::InvalidRequest(
            "ACP resource URI exceeds the size limit".into(),
        ));
    }
    Ok(uri)
}

fn reserve_resource(budget: &mut PromptBudget, bytes: usize) -> Result<(), UpstreamError> {
    if bytes > MAX_RESOURCE_BYTES
        || budget.resource_bytes.saturating_add(bytes) > MAX_PROMPT_RESOURCE_BYTES
    {
        return Err(UpstreamError::InvalidRequest(
            "ACP embedded context exceeds the prompt budget".into(),
        ));
    }
    budget.resource_bytes = budget.resource_bytes.saturating_add(bytes);
    Ok(())
}

fn required_string(params: &Value, field: &str) -> Result<String, UpstreamError> {
    params
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| UpstreamError::InvalidRequest(format!("ACP request has no {field}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_text_image_and_bounded_resource_blocks() {
        let mapped = turn_start_request(
            &json!({
                "sessionId": "thread",
                "prompt": [
                    {"type": "text", "text": "hello"},
                    {"type": "image", "data": "AA==", "mimeType": "image/png"},
                    {"type": "resource_link", "uri": "file:///doc", "name": "doc"},
                    {"type": "resource", "resource": {"uri": "memory://note", "text": "body"}}
                ]
            }),
            CapabilitySet::all(),
        )
        .expect("prompt maps");
        assert_eq!(mapped["params"]["input"].as_array().unwrap().len(), 4);
        assert!(mapped["params"]["input"][2]["text"]
            .as_str()
            .unwrap()
            .contains("file:///doc"));
    }

    #[test]
    fn rejects_binary_or_oversized_embedded_context() {
        assert!(turn_start_request(
            &json!({
                "sessionId": "thread",
                "prompt": [{"type": "resource", "resource": {"uri": "x", "blob": "AA=="}}]
            }),
            CapabilitySet::all(),
        )
        .is_err());
        let oversized = "x".repeat(MAX_RESOURCE_BYTES + 1);
        assert!(turn_start_request(
            &json!({
                "sessionId": "thread",
                "prompt": [{"type": "resource", "resource": {"uri": "x", "text": oversized}}]
            }),
            CapabilitySet::all(),
        )
        .is_err());
    }

    #[test]
    fn rejects_image_blocks_without_image_capability() {
        let prompt_only = CapabilitySet::empty().with(Capability::Prompt);
        assert!(turn_start_request(
            &json!({
                "sessionId": "thread",
                "prompt": [{"type": "image", "data": "AA==", "mimeType": "image/png"}]
            }),
            prompt_only,
        )
        .is_err());
        assert!(turn_start_request(
            &json!({
                "sessionId": "thread",
                "prompt": [{"type": "text", "text": "hello"}]
            }),
            prompt_only,
        )
        .is_ok());
    }
}
