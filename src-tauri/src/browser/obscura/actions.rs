use serde_json::{json, Value};

use super::super::agent_browser_input::managed_semantic_command;
use super::super::agent_tool_command::validated_command;
use super::super::agent_tool_support::{
    invalid_argument, optional_bool, optional_string, required_string, MAX_KEY_CHARS,
    MAX_SELECTOR_CHARS, MAX_TEXT_CHARS,
};
use super::super::tab_actions::validated_url;
use super::super::BrowserError;

const MAX_URL_CHARS: usize = 8_192;
const MAX_SNAPSHOT_DEPTH: u64 = 64;
const DEFAULT_SCROLL_PIXELS: u64 = 600;
const MAX_SCROLL_PIXELS: u64 = 10_000;
const DEFAULT_WAIT_MS: u64 = 1_000;

pub(super) enum Action {
    Open(String),
    Script(Vec<String>),
    Wait(String),
    Screenshot,
    Close,
    Managed,
}

pub(super) fn prepare(action: &str, input: &Value) -> Result<Action, BrowserError> {
    match action {
        "open" => {
            let new_tab = optional_bool(input, "new_tab")?.unwrap_or(false);
            if new_tab && optional_string(input, "tab_id", 128)?.is_some() {
                return Err(invalid_argument(
                    "browser_open cannot combine tab_id with new_tab=true",
                ));
            }
            Ok(Action::Open(validated_url(required_string(
                input,
                "url",
                MAX_URL_CHARS,
            )?)?))
        }
        "snapshot" => snapshot(input),
        "read" => read(input),
        "click" | "fill" => interaction(action, input),
        "advanced" => {
            validated_command(input)?;
            Ok(Action::Managed)
        }
        "present" | "request_user_action" => Ok(Action::Managed),
        _ => prepare_navigation(action, input),
    }
}

fn prepare_navigation(action: &str, input: &Value) -> Result<Action, BrowserError> {
    match action {
        "press" => Ok(Action::Script(vec![
            "press".into(),
            required_string(input, "key", MAX_KEY_CHARS)?.into(),
        ])),
        "scroll" => scroll(input),
        "wait" => wait(input),
        "screenshot" => {
            if optional_bool(input, "full_page")? == Some(true)
                || optional_bool(input, "annotate")? == Some(true)
            {
                return Ok(Action::Managed);
            }
            Ok(Action::Screenshot)
        }
        "close_tab" => Ok(Action::Close),
        _ => Err(invalid_argument("Unsupported browser action")),
    }
}

fn snapshot(input: &Value) -> Result<Action, BrowserError> {
    optional_bool(input, "interactive")?;
    optional_bool(input, "compact")?;
    let mut args = vec!["snapshot".into()];
    if let Some(depth) = input.get("depth") {
        let depth = depth
            .as_u64()
            .filter(|depth| *depth <= MAX_SNAPSHOT_DEPTH)
            .ok_or_else(|| invalid_argument("Invalid browser snapshot depth"))?;
        args.extend(["--depth".into(), depth.to_string()]);
    }
    if optional_string(input, "selector", MAX_SELECTOR_CHARS)?.is_some() {
        return Ok(Action::Managed);
    }
    Ok(Action::Script(args))
}

fn read(input: &Value) -> Result<Action, BrowserError> {
    let mut args = vec!["read".into()];
    if optional_bool(input, "raw")? == Some(true) {
        args.push("--raw".into());
    }
    if optional_bool(input, "outline")? == Some(true) {
        args.push("--outline".into());
    }
    if optional_string(input, "filter", MAX_SELECTOR_CHARS)?.is_some() {
        return Ok(Action::Managed);
    }
    Ok(Action::Script(args))
}

fn interaction(action: &str, input: &Value) -> Result<Action, BrowserError> {
    if let Some(semantic) = managed_semantic_command(action, input)? {
        return Ok(Action::Script(validated_command(&semantic)?));
    }
    let selector = required_string(input, "selector", MAX_SELECTOR_CHARS)?;
    let mut args = vec![action.into(), selector.into()];
    if action == "fill" {
        args.push(
            optional_string(input, "text", MAX_TEXT_CHARS)?
                .ok_or_else(|| invalid_argument("Missing browser argument: text"))?
                .into(),
        );
    }
    Ok(Action::Script(args))
}

fn scroll(input: &Value) -> Result<Action, BrowserError> {
    let direction = required_string(input, "direction", 8)?;
    if !["up", "down", "left", "right"].contains(&direction) {
        return Err(invalid_argument("Invalid scroll direction"));
    }
    let amount = input
        .get("pixels")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SCROLL_PIXELS)
        .clamp(1, MAX_SCROLL_PIXELS);
    Ok(Action::Script(vec![
        "scroll".into(),
        direction.into(),
        amount.to_string(),
    ]))
}

fn wait(input: &Value) -> Result<Action, BrowserError> {
    if let Some(selector) = optional_string(input, "selector", MAX_SELECTOR_CHARS)? {
        let timeout = super::super::agent_timeout::requested_timeout(
            input,
            super::super::agent_tool_support::COMMAND_TIMEOUT,
        )?
        .as_millis();
        return Ok(Action::Wait(format!("new Promise((resolve, reject) => {{ const end = Date.now() + {timeout}; const poll = () => {{ if (document.querySelector({})) resolve({{matched:true}}); else if (Date.now() >= end) reject(new Error('Wait condition timed out')); else setTimeout(poll, 100); }}; poll(); }})", json!(selector))));
    }
    let value = input.get("milliseconds").or_else(|| input.get("ms"));
    let milliseconds = value
        .map(Value::as_u64)
        .unwrap_or(Some(DEFAULT_WAIT_MS))
        .filter(|value| *value <= super::super::agent_tool_support::MAX_WAIT_MS)
        .ok_or_else(|| invalid_argument("Invalid browser wait duration"))?;
    Ok(Action::Wait(format!("new Promise(resolve => setTimeout(() => resolve({{waited:{milliseconds}}}), {milliseconds}))")))
}
