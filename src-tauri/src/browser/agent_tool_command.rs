use std::time::Duration;

use serde_json::Value;

use super::agent_tool_actions::AgentCliRequest;
use super::agent_tool_cancellation::AgentToolContext;
use super::agent_tool_support::{
    invalid_argument, optional_bool, optional_string, required_string, COMMAND_TIMEOUT,
    MAX_SELECTOR_CHARS,
};
use super::error::BrowserError;
use super::manager::BrowserSessionManager;

const MAX_ARGUMENTS: usize = 48;
const MAX_ARGUMENT_CHARS: usize = 8_192;
const MAX_TOTAL_ARGUMENT_CHARS: usize = 64 * 1_024;
const MAX_COMMAND_TIMEOUT_MS: u64 = 120_000;

const RESERVED_GLOBAL_ARGUMENTS: &[&str] = &[
    "--action-policy",
    "--allowed-domains",
    "--args",
    "--auto-connect",
    "--allow-file-access",
    "--cdp",
    "--color-scheme",
    "--config",
    "--confirm-actions",
    "--confirm-interactive",
    "--download-path",
    "--engine",
    "--enable",
    "--executable-path",
    "--extension",
    "--headed",
    "--headers",
    "--hide-scrollbars",
    "--idle-timeout",
    "--ignore-https-errors",
    "--init-script",
    "--json",
    "--max-output",
    "--model",
    "--namespace",
    "--no-auto-dialog",
    "--no-pin-tab",
    "--pin-tab",
    "--profile",
    "--provider",
    "--proxy",
    "--proxy-bypass",
    "--restore",
    "--restore-check-fn",
    "--restore-check-text",
    "--restore-check-url",
    "--restore-save",
    "--screenshot-format",
    "--screenshot-quality",
    "--screenshot-dir",
    "--session",
    "--session-name",
    "--state",
    "--user-agent",
    "--webgpu",
    "--content-boundaries",
    "-p",
];

const ALLOWED_COMMANDS: &[&str] = &[
    "a11y",
    "back",
    "check",
    "clipboard",
    "console",
    "cookies",
    "dialog",
    "diff",
    "download",
    "dblclick",
    "drag",
    "errors",
    "eval",
    "find",
    "focus",
    "forward",
    "frame",
    "get",
    "highlight",
    "hover",
    "inspect",
    "is",
    "keydown",
    "keyboard",
    "keyup",
    "mouse",
    "network",
    "pdf",
    "profiler",
    "pushstate",
    "react",
    "read",
    "reload",
    "removeinitscript",
    "scrollintoview",
    "select",
    "set",
    "state",
    "storage",
    "swipe",
    "tap",
    "trace",
    "type",
    "uncheck",
    "upload",
    "vitals",
    "wait",
    "device",
];

impl BrowserSessionManager {
    pub(super) async fn agent_read(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let mut args = vec!["read".to_string()];
        if let Some(filter) = optional_string(input, "filter", MAX_SELECTOR_CHARS)? {
            args.extend(["--filter".to_string(), filter.to_string()]);
        }
        if optional_bool(input, "outline")? == Some(true) {
            args.push("--outline".to_string());
        }
        if optional_bool(input, "raw")? == Some(true) {
            args.push("--raw".to_string());
        }
        self.run_command(AgentCliRequest {
            context,
            tab_id,
            args,
            timeout: COMMAND_TIMEOUT,
        })
        .await
    }

    pub(super) async fn agent_command(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let command = required_string(input, "command", 64)?.to_ascii_lowercase();
        if !ALLOWED_COMMANDS.contains(&command.as_str()) {
            return Err(invalid_argument("Unsupported managed browser command"));
        }
        let mut args = vec![command];
        args.extend(parse_arguments(input)?);
        let timeout = command_timeout(input)?;
        self.run_command(AgentCliRequest {
            context,
            tab_id,
            args,
            timeout,
        })
        .await
    }

    async fn run_command(&self, request: AgentCliRequest<'_>) -> Result<Value, BrowserError> {
        let context = request.context;
        let tab_id = request.tab_id.to_string();
        let output = self.run_agent_cli(request).await?;
        self.agent_state(context, Some(&tab_id), Some(output)).await
    }
}

fn parse_arguments(input: &Value) -> Result<Vec<String>, BrowserError> {
    let Some(values) = input.get("arguments") else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .filter(|values| values.len() <= MAX_ARGUMENTS)
        .ok_or_else(|| invalid_argument("Invalid browser command arguments"))?;
    let mut total = 0_usize;
    values
        .iter()
        .map(|value| {
            let value = value
                .as_str()
                .filter(|value| !value.contains('\0'))
                .ok_or_else(|| invalid_argument("Invalid browser command argument"))?;
            if reserved_global_argument(value) {
                return Err(invalid_argument(
                    "Browser command cannot override managed runtime arguments",
                ));
            }
            let chars = value.chars().count();
            total = total.saturating_add(chars);
            if chars > MAX_ARGUMENT_CHARS || total > MAX_TOTAL_ARGUMENT_CHARS {
                return Err(invalid_argument("Browser command arguments are too large"));
            }
            Ok(value.to_string())
        })
        .collect()
}

fn reserved_global_argument(value: &str) -> bool {
    RESERVED_GLOBAL_ARGUMENTS.iter().any(|reserved| {
        value.eq_ignore_ascii_case(reserved)
            || value
                .get(..reserved.len().saturating_add(1))
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&format!("{reserved}=")))
    })
}

fn command_timeout(input: &Value) -> Result<Duration, BrowserError> {
    let timeout_ms = input
        .get("timeout_ms")
        .map(|value| value.as_u64())
        .unwrap_or(Some(COMMAND_TIMEOUT.as_millis() as u64))
        .filter(|value| (1..=MAX_COMMAND_TIMEOUT_MS).contains(value))
        .ok_or_else(|| invalid_argument("Invalid browser command timeout"))?;
    Ok(Duration::from_millis(timeout_ms))
}
