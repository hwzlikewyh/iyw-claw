use crate::app_error::{AppCommandError, AppErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentFallbackReason {
    Network,
    PolicyMissing,
    StorageUnavailable,
    DownloadUnavailable,
    RateLimited,
    VersionNotFound,
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
        Some("AGENT_VERSION_NOT_FOUND") => Some(AgentFallbackReason::VersionNotFound),
        _ => None,
    }
}

pub(crate) fn allowed(error: &AppCommandError, allow_policy_missing: bool) -> bool {
    classify(error).is_some_and(|reason| match reason {
        AgentFallbackReason::PolicyMissing => allow_policy_missing,
        AgentFallbackReason::VersionNotFound => false,
        _ => true,
    })
}

pub(crate) fn launch_allowed(error: &AppCommandError) -> bool {
    matches!(
        classify(error),
        Some(
            AgentFallbackReason::Network
                | AgentFallbackReason::PolicyMissing
                | AgentFallbackReason::StorageUnavailable
                | AgentFallbackReason::DownloadUnavailable
                | AgentFallbackReason::RateLimited
                | AgentFallbackReason::VersionNotFound
        )
    )
}
