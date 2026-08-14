use std::process::ExitStatus;

use crate::chat_channel::error::ChatChannelError;

const MAX_ERROR_CHARS: usize = 512;
const MESSAGE_PERMISSION_ERROR: &str = "当前企业暂不支持授权机器人「消息」使用权限";

pub(super) fn from_process_failure(
    status: ExitStatus,
    stdout: &str,
    stderr: &str,
    args: &[&str],
) -> ChatChannelError {
    let raw_detail = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    let auth_failure = is_auth_failure(raw_detail);
    let detail = sanitize_detail(raw_detail, args);
    if auth_failure {
        return ChatChannelError::AuthenticationFailed(format!(
            "wecom-cli exited with {status}: {}",
            auth_summary(raw_detail)
        ));
    }
    if detail.is_empty() {
        ChatChannelError::ConnectionFailed(format!("wecom-cli exited with {status}"))
    } else {
        ChatChannelError::ConnectionFailed(format!("wecom-cli exited with {status}: {detail}"))
    }
}

pub(super) fn from_provider_failure(code: i64, message: &str, args: &[&str]) -> ChatChannelError {
    let auth_failure = is_auth_failure(message);
    let detail = sanitize_detail(message, args);
    if auth_failure {
        return ChatChannelError::AuthenticationFailed(format!(
            "wecom-cli provider code {code}: {}",
            auth_summary(message)
        ));
    }
    let summary = if detail.is_empty() {
        format!("wecom-cli provider code {code}")
    } else {
        format!("wecom-cli provider code {code}: {detail}")
    };
    if is_auth_failure(&summary) {
        ChatChannelError::AuthenticationFailed(summary)
    } else {
        ChatChannelError::ConnectionFailed(summary)
    }
}

fn is_auth_failure(detail: &str) -> bool {
    let normalized = detail.to_ascii_lowercase();
    detail.contains(MESSAGE_PERMISSION_ERROR)
        || normalized.contains("unauthorized")
        || normalized.contains("not authorized")
        || normalized.contains("permission denied")
}

fn auth_summary(detail: &str) -> &'static str {
    if detail.contains(MESSAGE_PERMISSION_ERROR) {
        MESSAGE_PERMISSION_ERROR
    } else {
        "permission denied"
    }
}

fn sanitize_detail(detail: &str, args: &[&str]) -> String {
    let mut redacted = detail.to_string();
    for arg in args.iter().filter(|arg| arg.trim_start().starts_with('{')) {
        redacted = redacted.replace(arg, "[payload redacted]");
    }
    let normalized = redacted.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();
    if normalized.contains('{')
        || normalized.contains('}')
        || [
            "chatid",
            "userid",
            "content",
            "token",
            "cookie",
            "secret",
            "access_token",
            "payload",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return "[detail redacted]".to_string();
    }
    normalized.chars().take(MAX_ERROR_CHARS).collect()
}
