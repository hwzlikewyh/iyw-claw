//! Stable boundary for the optional Codex in-process integration.
//!
//! This crate deliberately has no dependency on the current ACP transport yet.
//! The application-facing contract lives here so the upstream Codex protocol
//! and runtime types do not leak into `src-tauri/src/acp`.

use std::fmt;

#[cfg(feature = "upstream-acp")]
mod acp_agent;
mod contracts;
#[cfg(feature = "upstream")]
mod helper_dispatch;
mod method_routes;
mod runtime;
mod server_requests;
mod sessions;
mod upstream;
#[cfg(feature = "upstream")]
mod upstream_backend;
#[cfg(feature = "upstream")]
mod upstream_start;

pub use contracts::{
    Capability, CapabilitySet, OwnershipError, RequestClass, SessionAccess, SessionBinding,
    SessionOwner, SessionOwnership,
};
#[cfg(feature = "upstream")]
pub use helper_dispatch::dispatch_from_process_args as dispatch_upstream_helper;
#[cfg(not(feature = "upstream"))]
pub const fn dispatch_upstream_helper() -> bool {
    false
}
#[cfg(feature = "upstream-acp")]
pub use acp_agent::CodexAcpAgent;
pub use method_routes::{
    client_method_policy, server_method_policy, MethodPolicy, MethodScope, TurnScope,
    UnsupportedMethod,
};
pub use runtime::{CodexHarness, HarnessError, LifecycleError, ServerRequestAdmissionError};
pub use server_requests::{
    AdmittedServerRequest, ServerRequestDescriptor, ServerRequestError, ServerRequestTarget,
    ServerRequestToken,
};
pub use sessions::{ActiveTurn, SessionError, SessionRegistry, TurnBinding};
pub use upstream::{UpstreamPin, UPSTREAM_PIN};
#[cfg(feature = "upstream")]
pub use upstream_backend::{UpstreamClient, UpstreamError, UpstreamEvent, UpstreamEventPoll};
#[cfg(feature = "upstream")]
pub use upstream_start::UpstreamStartArgs;

/// The lifecycle states a host can expose without leaking upstream details.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HarnessState {
    Starting,
    Ready,
    Failed,
    ShuttingDown,
    Stopped,
}

impl HarnessState {
    /// Whether the runtime can accept a new request.
    pub const fn accepts_requests(self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Whether the runtime has reached a terminal state.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Failed | Self::Stopped)
    }
}

/// Configuration needed by a Codex harness runtime.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HarnessConfig {
    /// Client identity sent in the Codex App Server initialize request.
    pub client_name: String,
    /// Client version sent in the initialize request.
    pub client_version: String,
    /// Whether the caller opts into Codex experimental APIs.
    pub experimental_api: bool,
    /// Bounded command/event queue capacity.
    pub channel_capacity: usize,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            client_name: "iyw-claw-codex-harness".to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            experimental_api: false,
            channel_capacity: 128,
        }
    }
}

impl HarnessConfig {
    /// Validate values that would otherwise create ambiguous protocol state.
    pub fn validate(&self) -> Result<(), HarnessConfigError> {
        if self.client_name.trim().is_empty() {
            return Err(HarnessConfigError::EmptyClientName);
        }
        if self.client_version.trim().is_empty() {
            return Err(HarnessConfigError::EmptyClientVersion);
        }
        if self.channel_capacity == 0 {
            return Err(HarnessConfigError::ZeroChannelCapacity);
        }
        Ok(())
    }
}

/// Configuration failures are kept independent from upstream error types.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HarnessConfigError {
    EmptyClientName,
    EmptyClientVersion,
    ZeroChannelCapacity,
}

impl fmt::Display for HarnessConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyClientName => "Codex harness client name is empty",
            Self::EmptyClientVersion => "Codex harness client version is empty",
            Self::ZeroChannelCapacity => "Codex harness channel capacity must be positive",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for HarnessConfigError {}
