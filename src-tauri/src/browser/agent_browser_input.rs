use serde_json::{json, Map, Value};

use super::agent_tool_support::{invalid_argument, optional_string};
use super::error::BrowserError;

pub(super) fn browser_action(input: &Value) -> Result<String, BrowserError> {
    optional_string(input, "action", 64)?
        .or_else(|| input.get("op").and_then(Value::as_str))
        .map(str::to_ascii_lowercase)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_argument("Missing browser argument: action"))
}

pub(super) fn legacy_input(tool: &str, input: &Value) -> Result<Value, BrowserError> {
    let action = legacy_action(tool)?;
    let mut map = input
        .as_object()
        .cloned()
        .ok_or_else(|| invalid_argument("Browser input must be an object"))?;
    map.insert("action".to_string(), Value::String(action.to_string()));
    Ok(Value::Object(map))
}

pub(super) fn requires_managed(action: &str) -> bool {
    matches!(action, "request_user_action" | "present" | "close_window")
}

pub(super) fn managed_input(action: &str, input: &Value) -> Result<Value, BrowserError> {
    let mut map = input
        .as_object()
        .cloned()
        .ok_or_else(|| invalid_argument("Browser input must be an object"))?;
    copy_alias(&mut map, "tab_id", "tabId");
    copy_alias(&mut map, "new_tab", "newTab");
    copy_alias(&mut map, "timeout_ms", "timeoutMs");
    copy_alias(&mut map, "full_page", "fullPage");
    if let Some(target) = map.get("target").cloned() {
        if let Some(selector) = target_string(&target) {
            map.insert("selector".to_string(), Value::String(selector));
        } else if !target.as_object().is_some_and(|value| {
            value
                .keys()
                .any(|key| matches!(key.as_str(), "role" | "name" | "text"))
        }) {
            return Err(invalid_argument(
                "Managed browser target requires ref, selector, role, name, or text",
            ));
        }
    }
    if action == "advanced" && map.get("command").is_none() {
        return Err(invalid_argument("Missing browser argument: command"));
    }
    Ok(Value::Object(map))
}

pub(super) fn managed_semantic_command(
    action: &str,
    input: &Value,
) -> Result<Option<Value>, BrowserError> {
    if !matches!(action, "click" | "fill") {
        return Ok(None);
    }
    let Some(target) = input.get("target").and_then(Value::as_object) else {
        return Ok(None);
    };
    if target.get("ref").is_some() || target.get("selector").is_some() {
        return Ok(None);
    }
    let role = target_value(target, "role", 128)?;
    let text = target_value(target, "text", 2_048)?;
    let name = target_value(target, "name", 2_048)?;
    let (locator, value) = if let Some(role) = role {
        ("role", role)
    } else if let Some(text) = text {
        ("text", text)
    } else {
        return Err(invalid_argument(
            "Managed semantic target requires role or text",
        ));
    };
    let mut arguments = vec![locator.to_string(), value.to_string(), action.to_string()];
    if action == "fill" {
        arguments.push(
            optional_string(input, "text", 32_768)?
                .ok_or_else(|| invalid_argument("Missing browser argument: text"))?
                .to_string(),
        );
    }
    if role.is_some() {
        if let Some(name) = name.or(text) {
            arguments.extend(["--name".to_string(), name.to_string()]);
        }
    }
    Ok(Some(json!({
        "tab_id": input.get("tab_id"),
        "command": "find",
        "arguments": arguments,
    })))
}

pub(super) fn add_fallback(value: Value, reason: Option<&str>) -> Value {
    let mut value = value;
    if let Some(map) = value.as_object_mut() {
        map.insert("provider".to_string(), Value::String("managed".to_string()));
        if let Some(reason) = reason {
            map.insert(
                "fallback".to_string(),
                json!({
                    "from": "opencli",
                    "to": "managed",
                    "reason": reason,
                }),
            );
        }
    }
    value
}

fn legacy_action(tool: &str) -> Result<&'static str, BrowserError> {
    match tool {
        "browser_list_tabs" => Ok("list_tabs"),
        "browser_open" => Ok("open"),
        "browser_snapshot" => Ok("snapshot"),
        "browser_read" => Ok("read"),
        "browser_click" => Ok("click"),
        "browser_fill" => Ok("fill"),
        "browser_press" => Ok("press"),
        "browser_scroll" => Ok("scroll"),
        "browser_wait" => Ok("wait"),
        "browser_screenshot" => Ok("screenshot"),
        "browser_close_tab" => Ok("close_tab"),
        "browser_request_user_action" => Ok("request_user_action"),
        "browser_present" => Ok("present"),
        "browser_close_window" => Ok("close_window"),
        "browser_command" => Ok("advanced"),
        _ => Err(invalid_argument("Unknown browser tool")),
    }
}

fn copy_alias(map: &mut Map<String, Value>, primary: &str, alias: &str) {
    if map.get(primary).is_none() {
        if let Some(value) = map.get(alias).cloned() {
            map.insert(primary.to_string(), value);
        }
    }
}

fn target_string(target: &Value) -> Option<String> {
    target
        .as_str()
        .or_else(|| target.get("ref").and_then(Value::as_str))
        .or_else(|| target.get("selector").and_then(Value::as_str))
        .map(str::to_string)
}

fn target_value<'a>(
    target: &'a Map<String, Value>,
    key: &str,
    max_chars: usize,
) -> Result<Option<&'a str>, BrowserError> {
    let Some(value) = target.get(key) else {
        return Ok(None);
    };
    value
        .as_str()
        .filter(|value| !value.is_empty() && !value.contains('\0'))
        .filter(|value| value.chars().count() <= max_chars)
        .map(Some)
        .ok_or_else(|| invalid_argument("Invalid managed browser target"))
}
