use std::time::Duration;

use serde_json::Value;

use super::agent_browser_request_support::{
    advanced_arguments, request, requested_target, requested_timeout, validate_target,
};
use super::agent_tool_support::{
    invalid_argument, optional_bool, optional_string, required_string,
};
use super::error::BrowserError;
use super::opencli::is_supported_advanced_command;

#[derive(Debug)]
pub(super) struct OpencliRequest {
    pub command: String,
    pub args: Vec<String>,
    pub target: Option<String>,
    pub timeout: Duration,
}

pub(super) fn opencli_request(
    action: &str,
    input: &Value,
    current_target: Option<&str>,
) -> Result<OpencliRequest, BrowserError> {
    let target = requested_target(input).or_else(|| current_target.map(str::to_string));
    let timeout = requested_timeout(input);
    match action {
        "open" => open_request(input, target, timeout),
        "snapshot" => snapshot_request(input, target, timeout),
        "read" => read_request(input, target, timeout),
        "click" | "fill" => action_request(action, input, target, timeout),
        "press" => press_request(input, target, timeout),
        "scroll" => scroll_request(input, target, timeout),
        "wait" => wait_request(input, target, timeout),
        "screenshot" => screenshot_request(input, target, timeout),
        "close_tab" => close_request(target, timeout),
        "advanced" => advanced_request(input, target, timeout),
        _ => Err(invalid_argument("Unsupported OpenCLI browser action")),
    }
}

fn open_request(
    input: &Value,
    target: Option<String>,
    timeout: Duration,
) -> Result<OpencliRequest, BrowserError> {
    let url = required_string(input, "url", 8_192)?.to_string();
    let new_tab = optional_bool(input, "new_tab")?
        .or_else(|| input.get("newTab").and_then(Value::as_bool))
        == Some(true);
    if new_tab {
        return request("tab", vec!["new".to_string(), url], None, timeout);
    }
    request("open", vec![url], target, timeout)
}

fn snapshot_request(
    input: &Value,
    target: Option<String>,
    timeout: Duration,
) -> Result<OpencliRequest, BrowserError> {
    let unsupported = input.get("selector").is_some()
        || input.get("depth").is_some()
        || input.get("interactive").and_then(Value::as_bool) == Some(false)
        || input.get("compact").and_then(Value::as_bool) == Some(false);
    if unsupported {
        return Err(invalid_argument(
            "OpenCLI snapshot does not support selector, depth, interactive=false, or compact=false",
        ));
    }
    request("state", Vec::new(), target, timeout)
}

fn read_request(
    input: &Value,
    target: Option<String>,
    timeout: Duration,
) -> Result<OpencliRequest, BrowserError> {
    if input.get("outline").and_then(Value::as_bool) == Some(true) {
        return Err(invalid_argument(
            "OpenCLI read does not support outline=true",
        ));
    }
    let selector = optional_string(input, "filter", 2_048)?;
    if input.get("raw").and_then(Value::as_bool) == Some(true) {
        let mut args = vec!["html".to_string()];
        if let Some(selector) = selector {
            args.extend(["--selector".to_string(), selector.to_string()]);
        }
        return request("get", args, target, timeout);
    }
    let mut args = Vec::new();
    if let Some(filter) = selector {
        args.extend(["--selector".to_string(), filter.to_string()]);
    }
    request("extract", args, target, timeout)
}

fn action_request(
    action: &str,
    input: &Value,
    target: Option<String>,
    timeout: Duration,
) -> Result<OpencliRequest, BrowserError> {
    let mut args = Vec::new();
    append_action_target(&mut args, input)?;
    if action == "fill" {
        args.push(
            optional_string(input, "text", 32_768)?
                .ok_or_else(|| invalid_argument("Missing browser argument: text"))?
                .to_string(),
        );
    }
    request(action, args, target, timeout)
}

fn press_request(
    input: &Value,
    target: Option<String>,
    timeout: Duration,
) -> Result<OpencliRequest, BrowserError> {
    request(
        "keys",
        vec![required_string(input, "key", 128)?.to_string()],
        target,
        timeout,
    )
}

fn scroll_request(
    input: &Value,
    target: Option<String>,
    timeout: Duration,
) -> Result<OpencliRequest, BrowserError> {
    let direction = required_string(input, "direction", 8)?;
    if !matches!(direction, "up" | "down") {
        return Err(invalid_argument("OpenCLI scroll supports up or down"));
    }
    let amount = input
        .get("pixels")
        .or_else(|| input.get("amount"))
        .and_then(Value::as_u64)
        .unwrap_or(600)
        .clamp(1, 10_000);
    request(
        "scroll",
        vec![
            direction.to_string(),
            "--amount".to_string(),
            amount.to_string(),
        ],
        target,
        timeout,
    )
}

fn wait_request(
    input: &Value,
    target: Option<String>,
    timeout: Duration,
) -> Result<OpencliRequest, BrowserError> {
    let args = if let Some(selector) = optional_string(input, "selector", 2_048)? {
        vec![
            "selector".to_string(),
            selector.to_string(),
            "--timeout".to_string(),
            timeout.as_millis().to_string(),
        ]
    } else {
        let milliseconds = input
            .get("milliseconds")
            .and_then(Value::as_u64)
            .unwrap_or(1_000)
            .clamp(1, 30_000);
        vec![
            "time".to_string(),
            (milliseconds as f64 / 1_000.0).to_string(),
        ]
    };
    request("wait", args, target, timeout)
}

fn screenshot_request(
    input: &Value,
    target: Option<String>,
    timeout: Duration,
) -> Result<OpencliRequest, BrowserError> {
    let mut args = Vec::new();
    if input
        .get("full_page")
        .or_else(|| input.get("fullPage"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        args.push("--full".to_string());
    }
    if input.get("annotate").and_then(Value::as_bool) == Some(true) {
        args.push("--annotate".to_string());
    }
    request("screenshot", args, target, timeout)
}

fn close_request(
    target: Option<String>,
    timeout: Duration,
) -> Result<OpencliRequest, BrowserError> {
    let target = target.ok_or_else(|| invalid_argument("Missing browser argument: tab_id"))?;
    request("tab", vec!["close".to_string(), target], None, timeout)
}

fn advanced_request(
    input: &Value,
    target: Option<String>,
    timeout: Duration,
) -> Result<OpencliRequest, BrowserError> {
    let command = required_string(input, "command", 64)?.to_ascii_lowercase();
    if !is_supported_advanced_command(&command) {
        return Err(invalid_argument(
            "Unsupported OpenCLI advanced browser command",
        ));
    }
    let args = advanced_arguments(input)?;
    request(&command, args, target, timeout)
}

fn append_action_target(args: &mut Vec<String>, input: &Value) -> Result<(), BrowserError> {
    let target = input.get("target");
    if let Some(value) = target.and_then(target_string) {
        validate_target(&value, 2_048)?;
        args.push(value);
        return Ok(());
    }
    if target.and_then(Value::as_object).is_some_and(|target| {
        target.get("name").is_some() && target.get("role").is_none() && target.get("text").is_none()
    }) {
        return Err(invalid_argument(
            "Browser target name requires role or text",
        ));
    }
    for key in ["role", "name", "text"] {
        if let Some(value) = target
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
        {
            validate_target(value, if key == "role" { 128 } else { 2_048 })?;
            args.extend([format!("--{key}"), value.to_string()]);
        }
    }
    if args.is_empty() {
        if let Some(selector) = input.get("selector").and_then(Value::as_str) {
            validate_target(selector, 2_048)?;
            args.push(selector.to_string());
        }
    }
    (!args.is_empty())
        .then_some(())
        .ok_or_else(|| invalid_argument("Missing browser argument: target"))
}

fn target_string(target: &Value) -> Option<String> {
    target
        .as_str()
        .or_else(|| target.get("ref").and_then(Value::as_str))
        .or_else(|| target.get("selector").and_then(Value::as_str))
        .map(str::to_string)
}
