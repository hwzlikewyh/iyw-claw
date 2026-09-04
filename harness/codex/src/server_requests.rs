//! Pending server-request ownership and single-settlement tracking.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::{MethodPolicy, RequestClass, SessionAccess, SessionBinding};

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct ServerRequestToken(u64);

impl ServerRequestToken {
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ServerRequestTarget {
    Global,
    Session(SessionBinding),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AdmittedServerRequest {
    pub token: ServerRequestToken,
    pub method: String,
    pub class: RequestClass,
    pub target: ServerRequestTarget,
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ServerRequestDescriptor {
    pub request_id: String,
    pub method: String,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ServerRequestError {
    DuplicateId,
    MissingThread(String),
    MissingTurn(String),
    UnknownToken(u64),
    WrongTarget(u64),
    StaleTarget(String),
}

impl fmt::Display for ServerRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId => formatter.write_str("duplicate Codex server request id"),
            Self::MissingThread(method) => write!(formatter, "{method} has no thread id"),
            Self::MissingTurn(method) => write!(formatter, "{method} has no turn id"),
            Self::UnknownToken(token) => write!(formatter, "unknown server request token: {token}"),
            Self::WrongTarget(token) => write!(formatter, "wrong server request target: {token}"),
            Self::StaleTarget(id) => write!(formatter, "stale server request target: {id}"),
        }
    }
}

impl std::error::Error for ServerRequestError {}

#[derive(Debug)]
struct PendingServerRequest {
    request_id: String,
    target: ServerRequestTarget,
}

#[derive(Debug, Default)]
pub(crate) struct PendingServerRequests {
    next_token: u64,
    request_ids: HashSet<String>,
    pending: HashMap<ServerRequestToken, PendingServerRequest>,
}

impl PendingServerRequests {
    pub(crate) fn admit(
        &mut self,
        descriptor: ServerRequestDescriptor,
        policy: MethodPolicy,
        target: ServerRequestTarget,
    ) -> Result<AdmittedServerRequest, ServerRequestError> {
        if !self.request_ids.insert(descriptor.request_id.clone()) {
            return Err(ServerRequestError::DuplicateId);
        }
        self.next_token = self.next_token.wrapping_add(1).max(1);
        let token = ServerRequestToken(self.next_token);
        self.pending.insert(
            token,
            PendingServerRequest {
                request_id: descriptor.request_id,
                target: target.clone(),
            },
        );
        Ok(AdmittedServerRequest {
            token,
            method: descriptor.method,
            class: policy.class,
            target,
            turn_id: descriptor.turn_id,
        })
    }

    pub(crate) fn take_global(
        &mut self,
        token: ServerRequestToken,
    ) -> Result<String, ServerRequestError> {
        self.take_if(token, |target| {
            matches!(target, ServerRequestTarget::Global)
        })
    }

    pub(crate) fn take_session(
        &mut self,
        token: ServerRequestToken,
        access: SessionAccess<'_>,
    ) -> Result<String, ServerRequestError> {
        self.take_if(token, |target| match target {
            ServerRequestTarget::Session(binding) => binding_matches(binding, access),
            ServerRequestTarget::Global => false,
        })
    }

    pub(crate) fn clear(&mut self) {
        self.request_ids.clear();
        self.pending.clear();
    }

    fn take_if(
        &mut self,
        token: ServerRequestToken,
        predicate: impl FnOnce(&ServerRequestTarget) -> bool,
    ) -> Result<String, ServerRequestError> {
        let pending = self
            .pending
            .get(&token)
            .ok_or(ServerRequestError::UnknownToken(token.value()))?;
        if !predicate(&pending.target) {
            return Err(ServerRequestError::WrongTarget(token.value()));
        }
        let pending = self
            .pending
            .remove(&token)
            .ok_or(ServerRequestError::UnknownToken(token.value()))?;
        self.request_ids.remove(&pending.request_id);
        Ok(pending.request_id)
    }
}

fn binding_matches(binding: &SessionBinding, access: SessionAccess<'_>) -> bool {
    binding.external_id == access.external_id
        && binding.connection_id == access.connection_id
        && binding.generation == access.generation
        && binding.runtime_fingerprint == access.runtime_fingerprint
}
