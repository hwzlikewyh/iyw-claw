use std::time::Duration;

use serde_json::{json, Value};

use super::opencli_failure::{classify_failure, parse_execution};
pub(super) use super::opencli_failure::{OpencliFailure, OpencliFailureKind};
use crate::commands::internet_tools::{opencli_is_installed, run_opencli};

const DOCTOR_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_SESSION_CHARS: usize = 96;
const OPENCLI_COMMANDS: &[&str] = &[
    "bind",
    "unbind",
    "open",
    "back",
    "scroll",
    "state",
    "frames",
    "screenshot",
    "console",
    "dialog",
    "analyze",
    "find",
    "get",
    "click",
    "type",
    "hover",
    "focus",
    "dblclick",
    "check",
    "uncheck",
    "upload",
    "drag",
    "fill",
    "select",
    "keys",
    "wait",
    "eval",
    "extract",
    "network",
    "init",
    "verify",
    "tab",
    "close",
];
const OPENCLI_ADVANCED_COMMANDS: &[&str] = &[
    "back",
    "scroll",
    "state",
    "frames",
    "screenshot",
    "console",
    "dialog",
    "analyze",
    "find",
    "get",
    "click",
    "type",
    "hover",
    "focus",
    "dblclick",
    "check",
    "uncheck",
    "upload",
    "drag",
    "fill",
    "select",
    "keys",
    "wait",
    "eval",
    "extract",
    "network",
];

pub(super) struct OpencliProvider;

impl OpencliProvider {
    pub async fn doctor() -> Result<Value, OpencliFailure> {
        let execution = run_opencli(&["doctor".to_string()], DOCTOR_TIMEOUT)
            .await
            .map_err(|message| {
                let code = if opencli_is_installed() {
                    "OPENCLI_RUNTIME_FAILED"
                } else {
                    "OPENCLI_NOT_INSTALLED"
                };
                classify_failure(code, message)
            })?;
        let value = parse_execution(&execution)?;
        Ok(json!({
            "provider": "opencli",
            "status": "ready",
            "installed": opencli_is_installed(),
            "output": value,
        }))
    }

    pub async fn invoke(
        session: &str,
        command: &str,
        args: &[String],
        target: Option<&str>,
        timeout: Duration,
    ) -> Result<OpencliResult, OpencliFailure> {
        let session = validate_session(session)?;
        let command = validate_command(command)?;
        let mut cli_args = vec!["browser".to_string(), session, command.to_string()];
        if let Some(target) = target {
            cli_args.extend(["--tab".to_string(), target.to_string()]);
        }
        cli_args.extend(args.iter().cloned());
        let execution = run_opencli(&cli_args, timeout)
            .await
            .map_err(|message| classify_failure("OPENCLI_RUNTIME_FAILED", message))?;
        let output = parse_execution(&execution)?;
        let target_id = extract_target_id(&output);
        Ok(OpencliResult { output, target_id })
    }
}

#[derive(Debug, Clone)]
pub(super) struct OpencliResult {
    pub output: Value,
    pub target_id: Option<String>,
}

fn validate_session(session: &str) -> Result<String, OpencliFailure> {
    if session.is_empty()
        || session.len() > MAX_SESSION_CHARS
        || session
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')))
    {
        return Err(classify_failure(
            "OPENCLI_INVALID_SESSION",
            "Invalid OpenCLI browser session name".to_string(),
        ));
    }
    Ok(session.to_string())
}

fn validate_command(command: &str) -> Result<&str, OpencliFailure> {
    OPENCLI_COMMANDS
        .iter()
        .copied()
        .find(|allowed| *allowed == command)
        .ok_or_else(|| {
            classify_failure(
                "OPENCLI_UNSUPPORTED_COMMAND",
                "Unsupported OpenCLI browser command".to_string(),
            )
        })
}

pub(super) fn is_supported_advanced_command(command: &str) -> bool {
    OPENCLI_ADVANCED_COMMANDS
        .iter()
        .any(|allowed| *allowed == command)
}

fn extract_target_id(value: &Value) -> Option<String> {
    let map = value.as_object()?;
    for key in ["targetId", "target_id", "page", "tabId", "tab_id"] {
        if let Some(target) = map.get(key).and_then(Value::as_str) {
            return Some(target.to_string());
        }
    }
    None
}
