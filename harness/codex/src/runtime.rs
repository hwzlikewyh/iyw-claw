//! Protocol-neutral lifecycle facade for Codex runtime backends.

use std::fmt;

use crate::contracts::{Capability, CapabilitySet, SessionAccess, SessionBinding};
use crate::method_routes::{server_method_policy, MethodScope, TurnScope, UnsupportedMethod};
use crate::server_requests::{
    AdmittedServerRequest, PendingServerRequests, ServerRequestDescriptor, ServerRequestError,
    ServerRequestTarget, ServerRequestToken,
};
use crate::sessions::{ActiveTurn, SessionError, SessionRegistry, TurnBinding};
use crate::{HarnessConfig, HarnessState};

#[derive(Debug)]
pub struct CodexHarness {
    config: HarnessConfig,
    state: HarnessState,
    capabilities: CapabilitySet,
    sessions: SessionRegistry,
    server_requests: PendingServerRequests,
}

impl CodexHarness {
    pub fn new(config: HarnessConfig) -> Result<Self, crate::HarnessConfigError> {
        config.validate()?;
        Ok(Self {
            config,
            state: HarnessState::Starting,
            capabilities: CapabilitySet::empty(),
            sessions: SessionRegistry::default(),
            server_requests: PendingServerRequests::default(),
        })
    }

    pub fn state(&self) -> HarnessState {
        self.state
    }

    pub fn config(&self) -> &HarnessConfig {
        &self.config
    }

    pub fn capabilities(&self) -> CapabilitySet {
        self.capabilities
    }

    pub fn mark_ready(&mut self, capabilities: CapabilitySet) -> Result<(), LifecycleError> {
        match self.state {
            HarnessState::Starting => {
                self.capabilities = capabilities;
                self.state = HarnessState::Ready;
                Ok(())
            }
            state => Err(LifecycleError::InvalidTransition {
                from: state,
                to: HarnessState::Ready,
            }),
        }
    }

    pub fn mark_failed(&mut self) {
        self.sessions.clear();
        self.server_requests.clear();
        self.state = HarnessState::Failed;
    }

    pub fn bind_session(
        &mut self,
        binding: SessionBinding,
        capabilities: CapabilitySet,
    ) -> Result<(), HarnessError> {
        self.require_ready()?;
        if !capabilities.is_subset_of(self.capabilities) {
            return Err(HarnessError::CapabilityEscalation);
        }
        self.sessions
            .bind(binding, capabilities)
            .map_err(Into::into)
    }

    pub fn validate_session_binding(
        &self,
        binding: &SessionBinding,
        capabilities: CapabilitySet,
    ) -> Result<(), HarnessError> {
        self.require_ready()?;
        if !capabilities.is_subset_of(self.capabilities) {
            return Err(HarnessError::CapabilityEscalation);
        }
        self.sessions
            .ensure_bindable(binding, capabilities)
            .map_err(Into::into)
    }

    pub fn begin_turn(
        &mut self,
        access: SessionAccess<'_>,
        binding: TurnBinding,
    ) -> Result<(), HarnessError> {
        self.require_session_capability(access, Capability::Prompt)?;
        self.sessions
            .begin_turn(access, binding)
            .map_err(Into::into)
    }

    pub fn ensure_can_begin_turn(&self, access: SessionAccess<'_>) -> Result<(), HarnessError> {
        self.require_session_capability(access, Capability::Prompt)?;
        self.sessions
            .ensure_no_active_turn(access)
            .map_err(Into::into)
    }

    pub fn steer_turn(
        &self,
        access: SessionAccess<'_>,
        expected_turn_id: &str,
    ) -> Result<&ActiveTurn, HarnessError> {
        self.require_session_capability(access, Capability::Steering)?;
        self.sessions
            .steer(access, expected_turn_id)
            .map_err(Into::into)
    }

    pub fn cancel_turn(
        &mut self,
        access: SessionAccess<'_>,
        expected_turn_id: &str,
    ) -> Result<bool, HarnessError> {
        self.require_session_capability(access, Capability::Cancellation)?;
        self.sessions
            .cancel(access, expected_turn_id)
            .map_err(Into::into)
    }

    pub fn complete_turn(
        &mut self,
        access: SessionAccess<'_>,
        turn_id: &str,
    ) -> Result<ActiveTurn, HarnessError> {
        self.sessions.complete(access, turn_id).map_err(Into::into)
    }

    pub fn begin_shutdown(&mut self) -> Result<(), LifecycleError> {
        match self.state {
            HarnessState::Ready => {
                self.state = HarnessState::ShuttingDown;
                self.sessions.clear();
                self.server_requests.clear();
                Ok(())
            }
            state => Err(LifecycleError::InvalidTransition {
                from: state,
                to: HarnessState::ShuttingDown,
            }),
        }
    }

    pub fn finish_shutdown(&mut self) -> Result<(), LifecycleError> {
        if self.state != HarnessState::ShuttingDown {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                to: HarnessState::Stopped,
            });
        }
        self.state = HarnessState::Stopped;
        Ok(())
    }

    pub fn revoke_capability(&mut self, capability: Capability) -> Vec<ActiveTurn> {
        self.capabilities = self.capabilities.without(capability);
        self.server_requests.clear();
        self.sessions.revoke(capability)
    }

    fn require_ready(&self) -> Result<(), HarnessError> {
        self.state
            .accepts_requests()
            .then_some(())
            .ok_or(HarnessError::NotReady(self.state))
    }

    fn require_capability(&self, capability: Capability) -> Result<(), HarnessError> {
        self.require_ready()?;
        self.capabilities
            .contains(capability)
            .then_some(())
            .ok_or(HarnessError::CapabilityDenied(capability))
    }

    fn require_session_capability(
        &self,
        access: SessionAccess<'_>,
        capability: Capability,
    ) -> Result<(), HarnessError> {
        self.require_capability(capability)?;
        self.sessions
            .require_capability(access, capability)
            .map_err(Into::into)
    }

    pub fn validate_session(&self, access: SessionAccess<'_>) -> Result<(), HarnessError> {
        self.require_ready()?;
        self.sessions
            .validate(access)
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn validate_turn(
        &self,
        access: SessionAccess<'_>,
        expected_turn_id: &str,
    ) -> Result<(), HarnessError> {
        self.require_ready()?;
        self.sessions
            .active_turn(access, expected_turn_id)
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn validate_capability(
        &self,
        access: SessionAccess<'_>,
        capability: Capability,
    ) -> Result<(), HarnessError> {
        self.require_session_capability(access, capability)
    }

    pub fn validate_runtime_capability(&self, capability: Capability) -> Result<(), HarnessError> {
        self.require_capability(capability)
    }

    pub fn validate_session_capabilities(
        &self,
        capabilities: CapabilitySet,
    ) -> Result<(), HarnessError> {
        self.require_ready()?;
        capabilities
            .is_subset_of(self.capabilities)
            .then_some(())
            .ok_or(HarnessError::CapabilityEscalation)
    }

    pub fn binding(&self, external_id: &str) -> Option<SessionBinding> {
        self.sessions.binding(external_id)
    }

    pub fn active_turn_for(&self, external_id: &str) -> Option<ActiveTurn> {
        self.sessions.active_turn_for(external_id)
    }

    pub fn admit_server_request(
        &mut self,
        descriptor: ServerRequestDescriptor,
    ) -> Result<AdmittedServerRequest, ServerRequestAdmissionError> {
        self.require_ready()?;
        let policy = server_method_policy(&descriptor.method)?;
        let target = match policy.scope {
            MethodScope::Global => {
                if let Some(capability) = policy.capability {
                    self.require_capability(capability)?;
                }
                ServerRequestTarget::Global
            }
            MethodScope::Session => self.session_request_target(&descriptor, policy)?,
        };
        self.server_requests
            .admit(descriptor, policy, target)
            .map_err(Into::into)
    }

    pub fn take_global_server_request(
        &mut self,
        token: ServerRequestToken,
    ) -> Result<String, ServerRequestAdmissionError> {
        self.require_ready()?;
        self.server_requests.take_global(token).map_err(Into::into)
    }

    pub fn take_session_server_request(
        &mut self,
        access: SessionAccess<'_>,
        token: ServerRequestToken,
    ) -> Result<String, ServerRequestAdmissionError> {
        self.validate_session(access)?;
        self.server_requests
            .take_session(token, access)
            .map_err(Into::into)
    }

    fn session_request_target(
        &self,
        descriptor: &ServerRequestDescriptor,
        policy: crate::MethodPolicy,
    ) -> Result<ServerRequestTarget, ServerRequestAdmissionError> {
        let thread_id = descriptor
            .thread_id
            .as_deref()
            .ok_or_else(|| ServerRequestError::MissingThread(descriptor.method.clone()))?;
        let binding = self
            .sessions
            .binding(thread_id)
            .ok_or_else(|| ServerRequestError::StaleTarget(thread_id.to_string()))?;
        let access = SessionAccess {
            external_id: &binding.external_id,
            connection_id: &binding.connection_id,
            generation: binding.generation,
            runtime_fingerprint: &binding.runtime_fingerprint,
        };
        if let Some(capability) = policy.capability {
            self.require_session_capability(access, capability)?;
        }
        self.validate_server_turn(descriptor, access, policy.turn)?;
        Ok(ServerRequestTarget::Session(binding))
    }

    fn validate_server_turn(
        &self,
        descriptor: &ServerRequestDescriptor,
        access: SessionAccess<'_>,
        turn_scope: TurnScope,
    ) -> Result<(), ServerRequestAdmissionError> {
        match (turn_scope, descriptor.turn_id.as_deref()) {
            (TurnScope::Required, None) => {
                Err(ServerRequestError::MissingTurn(descriptor.method.clone()).into())
            }
            (TurnScope::Required | TurnScope::Optional, Some(turn_id)) => {
                self.validate_turn(access, turn_id).map_err(Into::into)
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug)]
pub enum ServerRequestAdmissionError {
    Harness(HarnessError),
    Request(ServerRequestError),
    Unsupported(UnsupportedMethod),
}

impl fmt::Display for ServerRequestAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Harness(error) => error.fmt(formatter),
            Self::Request(error) => error.fmt(formatter),
            Self::Unsupported(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ServerRequestAdmissionError {}

impl From<HarnessError> for ServerRequestAdmissionError {
    fn from(error: HarnessError) -> Self {
        Self::Harness(error)
    }
}

impl From<ServerRequestError> for ServerRequestAdmissionError {
    fn from(error: ServerRequestError) -> Self {
        Self::Request(error)
    }
}

impl From<UnsupportedMethod> for ServerRequestAdmissionError {
    fn from(error: UnsupportedMethod) -> Self {
        Self::Unsupported(error)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LifecycleError {
    InvalidTransition {
        from: HarnessState,
        to: HarnessState,
    },
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { from, to } => {
                write!(formatter, "invalid harness transition: {from:?} -> {to:?}")
            }
        }
    }
}

impl std::error::Error for LifecycleError {}

#[derive(Debug)]
pub enum HarnessError {
    NotReady(HarnessState),
    CapabilityDenied(Capability),
    CapabilityEscalation,
    Session(SessionError),
}

impl fmt::Display for HarnessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotReady(state) => write!(formatter, "harness is not ready: {state:?}"),
            Self::CapabilityDenied(capability) => {
                write!(
                    formatter,
                    "harness capability is not enabled: {capability:?}"
                )
            }
            Self::CapabilityEscalation => {
                formatter.write_str("session capabilities exceed runtime capabilities")
            }
            Self::Session(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for HarnessError {}

impl From<SessionError> for HarnessError {
    fn from(error: SessionError) -> Self {
        Self::Session(error)
    }
}
