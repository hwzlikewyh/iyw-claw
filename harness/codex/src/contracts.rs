//! Protocol-neutral contracts used by the Codex adapter.
//!
//! These types intentionally do not contain ACP or Codex App Server values.
//! They make ownership and lifecycle checks testable before either wire
//! protocol is connected.

use std::collections::HashMap;
use std::fmt;

/// Stable identity for one application-owned Codex session.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SessionBinding {
    pub connection_id: String,
    pub external_id: String,
    pub conversation_id: Option<i32>,
    pub generation: u64,
    pub runtime_fingerprint: String,
}

impl SessionBinding {
    pub fn new(
        owner: SessionOwner,
        external_id: impl Into<String>,
        runtime_fingerprint: impl Into<String>,
    ) -> Result<Self, OwnershipError> {
        let connection_id = non_empty(owner.connection_id, "connection id")?;
        let external_id = non_empty(external_id.into(), "external session id")?;
        let runtime_fingerprint = non_empty(runtime_fingerprint.into(), "runtime fingerprint")?;
        Ok(Self {
            connection_id,
            external_id,
            conversation_id: owner.conversation_id,
            generation: owner.generation,
            runtime_fingerprint,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SessionOwner {
    pub connection_id: String,
    pub conversation_id: Option<i32>,
    pub generation: u64,
}

impl SessionOwner {
    pub fn new(
        connection_id: impl Into<String>,
        conversation_id: Option<i32>,
        generation: u64,
    ) -> Result<Self, OwnershipError> {
        Ok(Self {
            connection_id: non_empty(connection_id.into(), "connection id")?,
            conversation_id,
            generation,
        })
    }

    pub fn validate(&self) -> Result<(), OwnershipError> {
        non_empty(self.connection_id.clone(), "connection id").map(|_| ())
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SessionAccess<'a> {
    pub external_id: &'a str,
    pub connection_id: &'a str,
    pub generation: u64,
    pub runtime_fingerprint: &'a str,
}

/// Owner table for request routing and stale-session rejection.
#[derive(Debug, Default)]
pub struct SessionOwnership {
    sessions: HashMap<String, SessionBinding>,
}

impl SessionOwnership {
    pub fn bind(&mut self, binding: SessionBinding) -> Result<(), OwnershipError> {
        if let Some(existing) = self.sessions.get(&binding.external_id) {
            if existing != &binding {
                return Err(OwnershipError::AlreadyBound(binding.external_id));
            }
            return Ok(());
        }
        self.sessions.insert(binding.external_id.clone(), binding);
        Ok(())
    }

    pub fn validate(&self, access: SessionAccess<'_>) -> Result<&SessionBinding, OwnershipError> {
        let binding = self
            .sessions
            .get(access.external_id)
            .ok_or_else(|| OwnershipError::UnknownSession(access.external_id.to_string()))?;
        if binding.connection_id != access.connection_id
            || binding.generation != access.generation
            || binding.runtime_fingerprint != access.runtime_fingerprint
        {
            return Err(OwnershipError::StaleSession(access.external_id.to_string()));
        }
        Ok(binding)
    }

    pub fn remove(&mut self, access: SessionAccess<'_>) -> bool {
        self.validate(access).is_ok() && self.sessions.remove(access.external_id).is_some()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn get(&self, external_id: &str) -> Option<&SessionBinding> {
        self.sessions.get(external_id)
    }

    pub fn clear(&mut self) {
        self.sessions.clear();
    }
}

/// Request classes used to apply the same policy to both wire protocols.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RequestClass {
    ReadOnly,
    Prompt,
    Cancellation,
    PermissionResponse,
    Configuration,
    Shutdown,
}

impl RequestClass {
    pub const fn may_change_state(self) -> bool {
        !matches!(self, Self::ReadOnly)
    }
}

/// Capabilities which must be advertised before a harness route is usable.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Capability {
    Prompt,
    Cancellation,
    Steering,
    Permission,
    Mcp,
    Skills,
    Images,
    Subagents,
    Terminal,
    Filesystem,
    Goals,
    Queue,
    Configuration,
}

/// Compact capability set advertised by one runtime host.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct CapabilitySet(u16);

impl CapabilitySet {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn all() -> Self {
        Self((1 << CAPABILITY_COUNT) - 1)
    }

    pub const fn contains(self, capability: Capability) -> bool {
        self.0 & capability.bit() != 0
    }

    pub const fn with(self, capability: Capability) -> Self {
        Self(self.0 | capability.bit())
    }

    pub const fn without(self, capability: Capability) -> Self {
        Self(self.0 & !capability.bit())
    }

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn is_subset_of(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }
}

const CAPABILITY_COUNT: u16 = 13;

impl Capability {
    const fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum OwnershipError {
    EmptyField(&'static str),
    AlreadyBound(String),
    UnknownSession(String),
    StaleSession(String),
}

impl fmt::Display for OwnershipError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(formatter, "{field} is empty"),
            Self::AlreadyBound(id) => write!(formatter, "session is already bound: {id}"),
            Self::UnknownSession(id) => write!(formatter, "session is unknown: {id}"),
            Self::StaleSession(id) => write!(formatter, "session binding is stale: {id}"),
        }
    }
}

impl std::error::Error for OwnershipError {}

fn non_empty(value: String, field: &'static str) -> Result<String, OwnershipError> {
    (!value.trim().is_empty())
        .then_some(value)
        .ok_or(OwnershipError::EmptyField(field))
}
