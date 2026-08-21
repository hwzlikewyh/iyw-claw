use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AcpError {
    #[error("agent process failed to spawn: {0}")]
    SpawnFailed(String),
    #[error("connection not found: {0}")]
    ConnectionNotFound(String),
    #[error("ACP protocol error: {0}")]
    Protocol(String),
    #[error("agent process exited unexpectedly")]
    ProcessExited,
    #[error("Agent capability denied: {0}")]
    CapabilityDenied(String),
    /// A prompt arrived while this connection already had a turn in flight.
    /// The connection loop processes one turn at a time; a second concurrent
    /// prompt (e.g. two co-controlling clients sending near-simultaneously)
    /// is rejected here rather than silently dropped after a false success.
    /// The frontend recognizes this (via the stable Display text, carried as
    /// the error message on both transports) and re-queues the draft in the
    /// message queue above the input box instead of surfacing an error.
    #[error("turn already in progress for this connection")]
    TurnInProgress,
    /// Live feedback was submitted while no turn was in flight. Feedback only
    /// makes sense while the agent is working (it is pulled mid-turn via the
    /// `check_user_feedback` MCP tool); with no active turn there is nothing to
    /// steer. The frontend recognizes this (stable Display text) and falls back
    /// to sending the text as an ordinary prompt instead.
    #[error("no active turn to send feedback to")]
    NoActiveTurn,
    /// Live feedback was submitted while the feature is disabled. The settings
    /// toggle gates both MCP tool injection and the UI affordance; this is the
    /// backend's defense-in-depth for a direct/stale call.
    #[error("live feedback is disabled")]
    FeedbackDisabled,
    /// The submitted feedback note is empty or exceeds the per-note size bound.
    /// The full text rides in the broadcast event + snapshot + MCP response, so
    /// a sanity bound keeps a single pathological note from bloating them.
    #[error("invalid feedback: {0}")]
    InvalidFeedback(String),
    #[error("binary download failed: {0}")]
    DownloadFailed(String),
    #[error("platform not supported: {0}")]
    PlatformNotSupported(String),
    #[error("{0}")]
    SdkNotInstalled(String),
    #[error("Sign in to iyw-claw before installing or using Agents")]
    AuthenticationRequired,
    #[error("Agent local initialization timed out. Check Agent Settings and startup logs.")]
    InitializeTimeout,
    #[error("Agent did not publish its configurable options within 60 seconds. The probe was aborted; the agent may be slow, idle, or not ACP-compliant — try again or check the agent binary.")]
    ProbeTimedOut,
    #[error("failed to render built-in Agent prompt: {0}")]
    BuiltinPromptRender(String),
    #[error("failed to inject built-in Agent prompt: {0}")]
    BuiltinPromptInjection(String),
    #[error("built-in Agent prompt bridge is busy: {0}")]
    BuiltinPromptBridgeBusy(String),
    #[error("failed to clean up built-in Agent prompt bridge: {0}")]
    BuiltinPromptBridgeCleanup(String),
    #[error("built-in MCP is unavailable: {0}")]
    BuiltinMcpUnavailable(String),
}

impl AcpError {
    pub fn from_capability_error(error: crate::app_error::AppCommandError) -> Self {
        let code = if error.code == crate::app_error::AppErrorCode::PermissionDenied {
            error
                .detail
                .unwrap_or_else(|| "remote_policy_denied".to_string())
        } else {
            "remote_policy_missing".to_string()
        };
        Self::CapabilityDenied(code)
    }

    pub fn protocol(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let sanitized = sanitize_protocol_message(&raw);

        if is_executable_format_error(&sanitized) {
            return Self::Protocol(
                "Agent executable appears incompatible or corrupted. Please retry to re-download it."
                    .into(),
            );
        }

        Self::Protocol(sanitized)
    }

    /// Stable machine-readable identifier for this error kind.
    ///
    /// Returned to the frontend alongside the human-readable message so
    /// the UI can render a localized message based on the code instead
    /// of parsing English text. `None` means "no stable code — show the
    /// raw message as a fallback".
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::SdkNotInstalled(_) => Some("sdk_not_installed"),
            Self::AuthenticationRequired => Some("authentication_required"),
            Self::PlatformNotSupported(_) => Some("platform_not_supported"),
            Self::InitializeTimeout => Some("initialize_timeout"),
            Self::ProbeTimedOut => Some("probe_timed_out"),
            Self::ProcessExited => Some("process_exited"),
            Self::CapabilityDenied(_) => Some("capability_denied"),
            Self::TurnInProgress => Some("turn_in_progress"),
            Self::NoActiveTurn => Some("no_active_turn"),
            Self::FeedbackDisabled => Some("feedback_disabled"),
            Self::InvalidFeedback(_) => Some("invalid_feedback"),
            Self::SpawnFailed(_) => Some("spawn_failed"),
            Self::DownloadFailed(_) => Some("download_failed"),
            Self::ConnectionNotFound(_) => Some("connection_not_found"),
            Self::BuiltinPromptRender(_) => Some("builtin_prompt_render_failed"),
            Self::BuiltinPromptInjection(_) => Some("builtin_prompt_injection_failed"),
            Self::BuiltinPromptBridgeBusy(_) => Some("builtin_prompt_bridge_busy"),
            Self::BuiltinPromptBridgeCleanup(_) => Some("builtin_prompt_bridge_cleanup_failed"),
            Self::BuiltinMcpUnavailable(_) => Some("builtin_mcp_unavailable"),
            Self::Protocol(_) => None,
        }
    }
}

impl Serialize for AcpError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

fn sanitize_protocol_message(raw: &str) -> String {
    let without_spawned_at = regex::Regex::new(r#"\s*,?\s*"spawned_at"\s*:\s*"[^"]*"\s*,?"#)
        .ok()
        .map(|re| re.replace_all(raw, "").into_owned())
        .unwrap_or_else(|| raw.to_string());

    let without_dangling_comma = regex::Regex::new(r#",\s*([}\]])"#)
        .ok()
        .map(|re| re.replace_all(&without_spawned_at, "$1").into_owned())
        .unwrap_or(without_spawned_at);

    regex::Regex::new(r#"/(?:Users|home)/[^"\s]+"#)
        .ok()
        .map(|re| {
            re.replace_all(&without_dangling_comma, "<local-path>")
                .into_owned()
        })
        .unwrap_or(without_dangling_comma)
}

fn is_executable_format_error(message: &str) -> bool {
    let lowered = message.to_lowercase();
    lowered.contains("malformed mach-o file")
        || lowered.contains("exec format error")
        || lowered.contains("bad cpu type in executable")
        || lowered.contains("not a valid win32 application")
        || lowered.contains("is not a valid application for this os platform")
}
