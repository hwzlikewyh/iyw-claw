use serde_json::Value;

use super::error::{BrowserError, BrowserErrorCode};
use crate::commands::internet_tools::OpencliExecution;

const AUTH_CODE_TERMS: &[&str] = &[
    "auth_required",
    "login_required",
    "user_action",
    "interaction_required",
    "verification_required",
];
const BRIDGE_TERMS: &[&str] = &[
    "chrome",
    "extension",
    "bridge",
    "daemon",
    "cdp",
    "not connected",
    "debug port",
];
const TIMEOUT_TERMS: &[&str] = &["timeout", "timed out", "deadline", "command_timeout"];
const SELECTOR_TERMS: &[&str] = &[
    "selector",
    "not_found",
    "stale_ref",
    "ambiguous",
    "option_not_found",
];
const NETWORK_TERMS: &[&str] = &["network", "fetch", "econn", "http 5"];
const USER_ACTION_TERMS: &[&str] = &[
    "sign in",
    "log in",
    "mfa",
    "otp",
    "captcha",
    "device approval",
    "security confirmation",
    "human review",
    "验证码",
    "登录",
    "人工",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpencliFailureKind {
    UserAction,
    BridgeUnavailable,
    Selector,
    Network,
    Timeout,
    Runtime,
}

#[derive(Debug, Clone)]
pub(super) struct OpencliFailure {
    pub code: String,
    pub message: String,
    pub kind: OpencliFailureKind,
}

impl OpencliFailure {
    pub fn user_action(message: impl Into<String>) -> Self {
        Self {
            code: "OPENCLI_USER_ACTION_REQUIRED".to_string(),
            message: message.into(),
            kind: OpencliFailureKind::UserAction,
        }
    }

    pub fn is_user_action(&self) -> bool {
        self.kind == OpencliFailureKind::UserAction
    }

    pub fn browser_error(&self) -> BrowserError {
        let code = if self.code == "OPENCLI_NOT_INSTALLED" {
            BrowserErrorCode::OpencliNotInstalled
        } else if self.code == "OPENCLI_INVALID_ARGUMENT" {
            BrowserErrorCode::BrowserInvalidArgument
        } else {
            match self.kind {
                OpencliFailureKind::UserAction => BrowserErrorCode::OpencliUserActionRequired,
                OpencliFailureKind::BridgeUnavailable => BrowserErrorCode::OpencliBridgeUnavailable,
                OpencliFailureKind::Selector => BrowserErrorCode::OpencliSelectorFailed,
                OpencliFailureKind::Network => BrowserErrorCode::OpencliNetworkFailed,
                OpencliFailureKind::Timeout => BrowserErrorCode::OpencliTimeout,
                OpencliFailureKind::Runtime => BrowserErrorCode::OpencliRuntimeFailed,
            }
        };
        BrowserError::new(code, self.message.clone()).retryable(matches!(
            self.kind,
            OpencliFailureKind::BridgeUnavailable
                | OpencliFailureKind::Network
                | OpencliFailureKind::Timeout
        ))
    }
}

pub(super) fn parse_execution(execution: &OpencliExecution) -> Result<Value, OpencliFailure> {
    let value = parse_json(&execution.stdout).unwrap_or_else(|| {
        Value::String(if execution.stdout.trim().is_empty() {
            execution.stderr.trim().to_string()
        } else {
            execution.stdout.trim().to_string()
        })
    });
    if execution.success && value.get("error").is_none() {
        return Ok(value);
    }
    let fallback_message = value.to_string();
    let (code, message) = value
        .get("error")
        .map(|error| {
            (
                error
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("OPENCLI_RUNTIME_FAILED"),
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenCLI browser command failed"),
            )
        })
        .unwrap_or(("OPENCLI_RUNTIME_FAILED", fallback_message.as_str()));
    let code = match (code, execution.exit_code) {
        ("OPENCLI_RUNTIME_FAILED", Some(69)) => "OPENCLI_BRIDGE_UNAVAILABLE",
        ("OPENCLI_RUNTIME_FAILED", Some(75)) => "OPENCLI_TIMEOUT",
        ("OPENCLI_RUNTIME_FAILED", Some(77)) => "OPENCLI_AUTH_REQUIRED",
        (code, _) => code,
    };
    Err(classify_failure(code, message.to_string()))
}

pub(super) fn classify_failure(code: &str, message: String) -> OpencliFailure {
    let normalized_code = code.to_ascii_lowercase();
    let normalized = format!("{} {}", code, message).to_ascii_lowercase();
    let kind = failure_kind(&normalized_code, &normalized);
    OpencliFailure {
        code: code.to_string(),
        message,
        kind,
    }
}

fn failure_kind(code: &str, message: &str) -> OpencliFailureKind {
    if contains_any(code, AUTH_CODE_TERMS) {
        OpencliFailureKind::UserAction
    } else if contains_any(message, BRIDGE_TERMS) {
        OpencliFailureKind::BridgeUnavailable
    } else if contains_any(message, TIMEOUT_TERMS) {
        OpencliFailureKind::Timeout
    } else if contains_any(message, SELECTOR_TERMS) {
        OpencliFailureKind::Selector
    } else if contains_any(message, NETWORK_TERMS) {
        OpencliFailureKind::Network
    } else if contains_any(message, USER_ACTION_TERMS) {
        OpencliFailureKind::UserAction
    } else {
        OpencliFailureKind::Runtime
    }
}

fn parse_json(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok().or_else(|| {
        text.lines()
            .rev()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .find_map(|line| serde_json::from_str(line).ok())
    })
}

fn contains_any(value: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| value.contains(term))
}
