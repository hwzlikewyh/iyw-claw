use crate::app_error::{AppCommandError, AppErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentFallbackReason {
    Network,
    PolicyMissing,
    StorageUnavailable,
    DownloadUnavailable,
    RateLimited,
}

pub(crate) fn classify(error: &AppCommandError) -> Option<AgentFallbackReason> {
    if error.code == AppErrorCode::NetworkError {
        return Some(AgentFallbackReason::Network);
    }
    if error.code != AppErrorCode::InvalidInput {
        return None;
    }
    match error.detail.as_deref() {
        Some("AGENT_POLICY_MISSING") => Some(AgentFallbackReason::PolicyMissing),
        Some("AGENT_STORAGE_UNAVAILABLE") => Some(AgentFallbackReason::StorageUnavailable),
        Some("AGENT_DOWNLOAD_UNAVAILABLE") => Some(AgentFallbackReason::DownloadUnavailable),
        Some("AGENT_RATE_LIMITED") => Some(AgentFallbackReason::RateLimited),
        _ => None,
    }
}

pub(crate) fn allowed(error: &AppCommandError, allow_policy_missing: bool) -> bool {
    classify(error)
        .is_some_and(|reason| reason != AgentFallbackReason::PolicyMissing || allow_policy_missing)
}
