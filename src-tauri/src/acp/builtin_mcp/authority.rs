use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use super::features::{FeatureSnapshot, MemoryPermissions};
use crate::models::AgentType;

const MAX_ACTIVE_REQUESTS_PER_TOKEN: usize = 16;
const MAX_ACTIVE_STREAMS_PER_TOKEN: usize = 1;

#[derive(Debug)]
pub struct SessionIdentity {
    connection_id: String,
    cwd: PathBuf,
    agent_type: AgentType,
    gateway_server_name: String,
}

impl SessionIdentity {
    pub fn new(
        connection_id: String,
        cwd: PathBuf,
        agent_type: AgentType,
        gateway_server_name: String,
    ) -> Self {
        Self {
            connection_id,
            cwd,
            agent_type,
            gateway_server_name,
        }
    }
}

/// Host-minted authority; intentionally not deserializable from MCP input.
#[derive(Debug)]
pub struct SessionAuthority {
    identity: SessionIdentity,
    features: FeatureSnapshot,
    memory: MemoryPermissions,
    cancellation: CancellationToken,
}

impl SessionAuthority {
    pub fn new(
        identity: SessionIdentity,
        features: FeatureSnapshot,
        memory: MemoryPermissions,
    ) -> Self {
        Self {
            identity,
            features,
            memory,
            cancellation: CancellationToken::new(),
        }
    }

    pub fn with_parent_cancellation(mut self, parent: &CancellationToken) -> Self {
        self.cancellation = parent.child_token();
        self
    }

    pub fn connection_id(&self) -> &str {
        &self.identity.connection_id
    }

    pub fn cwd(&self) -> &Path {
        &self.identity.cwd
    }

    pub fn agent_type(&self) -> AgentType {
        self.identity.agent_type
    }

    pub fn gateway_server_name(&self) -> &str {
        &self.identity.gateway_server_name
    }

    pub fn features(&self) -> &FeatureSnapshot {
        &self.features
    }

    pub fn memory_permissions(&self) -> MemoryPermissions {
        self.memory
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    fn cancel(&self) {
        self.cancellation.cancel();
    }
}

#[derive(Debug, Clone)]
pub(super) struct SessionContext {
    authority: Arc<SessionAuthority>,
    request_capacity: Arc<Semaphore>,
    stream_capacity: Arc<Semaphore>,
}

impl SessionContext {
    pub(super) fn new(authority: SessionAuthority) -> Self {
        Self {
            authority: Arc::new(authority),
            request_capacity: Arc::new(Semaphore::new(MAX_ACTIVE_REQUESTS_PER_TOKEN)),
            stream_capacity: Arc::new(Semaphore::new(MAX_ACTIVE_STREAMS_PER_TOKEN)),
        }
    }

    pub(super) fn cancel(&self) {
        self.authority.cancel();
    }

    pub(super) fn try_acquire_request(&self) -> Option<OwnedSemaphorePermit> {
        Arc::clone(&self.request_capacity).try_acquire_owned().ok()
    }

    pub(super) fn try_acquire_stream(&self) -> Option<OwnedSemaphorePermit> {
        Arc::clone(&self.stream_capacity).try_acquire_owned().ok()
    }
}

impl Deref for SessionContext {
    type Target = SessionAuthority;

    fn deref(&self) -> &Self::Target {
        self.authority.as_ref()
    }
}
