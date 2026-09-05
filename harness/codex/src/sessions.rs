//! Generation-safe session and turn ownership for the harness boundary.

use std::collections::HashMap;
use std::fmt;

use crate::contracts::{
    Capability, CapabilitySet, OwnershipError, SessionAccess, SessionBinding, SessionOwnership,
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TurnBinding {
    pub thread_id: String,
    pub turn_id: String,
}

impl TurnBinding {
    pub fn new(
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Result<Self, SessionError> {
        Ok(Self {
            thread_id: non_empty(thread_id.into(), "thread id")?,
            turn_id: non_empty(turn_id.into(), "turn id")?,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ActiveTurn {
    pub thread_id: String,
    pub turn_id: String,
    pub generation: u64,
    pub cancelling: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SessionError {
    Ownership(OwnershipError),
    EmptyTurnId,
    TurnAlreadyActive(String),
    UnknownTurn(String),
    StaleTurn(String),
    CapabilityMismatch(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ownership(error) => error.fmt(formatter),
            Self::EmptyTurnId => formatter.write_str("turn id is empty"),
            Self::TurnAlreadyActive(id) => write!(formatter, "turn is already active: {id}"),
            Self::UnknownTurn(id) => write!(formatter, "turn is unknown: {id}"),
            Self::StaleTurn(id) => write!(formatter, "turn binding is stale: {id}"),
            Self::CapabilityMismatch(id) => {
                write!(
                    formatter,
                    "session capabilities changed during rebind: {id}"
                )
            }
        }
    }
}

impl std::error::Error for SessionError {}

impl From<OwnershipError> for SessionError {
    fn from(error: OwnershipError) -> Self {
        Self::Ownership(error)
    }
}

#[derive(Debug, Default)]
pub struct SessionRegistry {
    ownership: SessionOwnership,
    capabilities: HashMap<String, CapabilitySet>,
    active_turns: HashMap<String, ActiveTurn>,
}

impl SessionRegistry {
    pub fn ensure_bindable(
        &self,
        binding: &SessionBinding,
        capabilities: CapabilitySet,
    ) -> Result<(), SessionError> {
        if let Some(existing) = self.ownership.get(&binding.external_id) {
            if existing != binding {
                return Err(SessionError::Ownership(OwnershipError::AlreadyBound(
                    binding.external_id.clone(),
                )));
            }
            if self.capabilities.get(&binding.external_id).copied() != Some(capabilities) {
                return Err(SessionError::CapabilityMismatch(
                    binding.external_id.clone(),
                ));
            }
        }
        Ok(())
    }

    pub fn bind(
        &mut self,
        binding: SessionBinding,
        capabilities: CapabilitySet,
    ) -> Result<(), SessionError> {
        if self
            .capabilities
            .get(&binding.external_id)
            .is_some_and(|current| *current != capabilities)
        {
            return Err(SessionError::CapabilityMismatch(binding.external_id));
        }
        self.ownership.bind(binding.clone())?;
        self.capabilities
            .insert(binding.external_id.clone(), capabilities);
        Ok(())
    }

    pub fn validate(&self, access: SessionAccess<'_>) -> Result<&SessionBinding, SessionError> {
        Ok(self.ownership.validate(access)?)
    }

    pub fn begin_turn(
        &mut self,
        access: SessionAccess<'_>,
        binding: TurnBinding,
    ) -> Result<(), SessionError> {
        self.ensure_no_active_turn(access)?;
        if binding.thread_id != access.external_id {
            return Err(SessionError::StaleTurn(access.external_id.to_string()));
        }
        self.active_turns.insert(
            access.external_id.to_string(),
            ActiveTurn {
                thread_id: binding.thread_id,
                turn_id: binding.turn_id,
                generation: access.generation,
                cancelling: false,
            },
        );
        Ok(())
    }

    pub fn ensure_no_active_turn(&self, access: SessionAccess<'_>) -> Result<(), SessionError> {
        self.validate(access)?;
        (!self.active_turns.contains_key(access.external_id))
            .then_some(())
            .ok_or_else(|| SessionError::TurnAlreadyActive(access.external_id.to_string()))
    }

    pub fn steer(
        &self,
        access: SessionAccess<'_>,
        expected_turn_id: &str,
    ) -> Result<&ActiveTurn, SessionError> {
        self.validate(access)?;
        let turn = self
            .active_turns
            .get(access.external_id)
            .ok_or_else(|| SessionError::UnknownTurn(access.external_id.to_string()))?;
        (turn.generation == access.generation && turn.turn_id == expected_turn_id)
            .then_some(turn)
            .ok_or_else(|| SessionError::StaleTurn(access.external_id.to_string()))
    }

    pub fn active_turn(
        &self,
        access: SessionAccess<'_>,
        expected_turn_id: &str,
    ) -> Result<&ActiveTurn, SessionError> {
        self.steer(access, expected_turn_id)
    }

    pub fn active_turn_for(&self, external_id: &str) -> Option<ActiveTurn> {
        self.active_turns.get(external_id).cloned()
    }

    pub fn cancel(
        &mut self,
        access: SessionAccess<'_>,
        expected_turn_id: &str,
    ) -> Result<bool, SessionError> {
        self.validate(access)?;
        let turn = self
            .active_turns
            .get_mut(access.external_id)
            .ok_or_else(|| SessionError::UnknownTurn(access.external_id.to_string()))?;
        if turn.generation != access.generation || turn.turn_id != expected_turn_id {
            return Err(SessionError::StaleTurn(access.external_id.to_string()));
        }
        let changed = !turn.cancelling;
        turn.cancelling = true;
        Ok(changed)
    }

    pub fn complete(
        &mut self,
        access: SessionAccess<'_>,
        turn_id: &str,
    ) -> Result<ActiveTurn, SessionError> {
        self.validate(access)?;
        let turn = self
            .active_turns
            .get(access.external_id)
            .ok_or_else(|| SessionError::UnknownTurn(access.external_id.to_string()))?;
        if turn.generation != access.generation || turn.turn_id != turn_id {
            return Err(SessionError::StaleTurn(access.external_id.to_string()));
        }
        self.active_turns
            .remove(access.external_id)
            .ok_or_else(|| SessionError::UnknownTurn(access.external_id.to_string()))
    }

    pub fn capabilities(&self, external_id: &str) -> Option<CapabilitySet> {
        self.capabilities.get(external_id).copied()
    }

    pub fn binding(&self, external_id: &str) -> Option<SessionBinding> {
        self.ownership.get(external_id).cloned()
    }

    pub fn require_capability(
        &self,
        access: SessionAccess<'_>,
        capability: Capability,
    ) -> Result<(), SessionError> {
        self.validate(access)?;
        self.capabilities
            .get(access.external_id)
            .copied()
            .filter(|set| set.contains(capability))
            .map(|_| ())
            .ok_or_else(|| SessionError::CapabilityMismatch(access.external_id.to_string()))
    }

    pub fn revoke(&mut self, capability: Capability) -> Vec<ActiveTurn> {
        for capabilities in self.capabilities.values_mut() {
            *capabilities = capabilities.without(capability);
        }
        self.active_turns.drain().map(|(_, turn)| turn).collect()
    }

    pub fn remove(&mut self, access: SessionAccess<'_>) -> Result<bool, SessionError> {
        self.validate(access)?;
        self.active_turns.remove(access.external_id);
        self.capabilities.remove(access.external_id);
        Ok(self.ownership.remove(access))
    }

    pub fn clear(&mut self) {
        self.active_turns.clear();
        self.capabilities.clear();
        self.ownership.clear();
    }

    pub fn len(&self) -> usize {
        self.ownership.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ownership.is_empty()
    }
}

fn non_empty(value: String, field: &'static str) -> Result<String, SessionError> {
    (!value.trim().is_empty()).then_some(value).ok_or_else(|| {
        if field == "turn id" {
            SessionError::EmptyTurnId
        } else {
            SessionError::Ownership(OwnershipError::EmptyField(field))
        }
    })
}
