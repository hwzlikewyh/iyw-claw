use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::enforcement::denied;
use super::{Capability, CapabilityEnforcer, DenialCode};
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

const REVOCATION_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub struct CapabilityRevocationMonitor {
    enforcer: CapabilityEnforcer,
    agent_type: Option<AgentType>,
    capability: Capability,
    runtime_verified: bool,
    cancel_target: Option<CancellationToken>,
    revoked: CancellationToken,
    denial: Arc<StdMutex<Option<AppCommandError>>>,
    stop: CancellationToken,
    task: JoinHandle<()>,
}

impl CapabilityRevocationMonitor {
    pub(super) fn spawn(
        enforcer: CapabilityEnforcer,
        agent_type: Option<AgentType>,
        capability: Capability,
        runtime_verified: bool,
        cancel_target: Option<CancellationToken>,
    ) -> Self {
        let revoked = CancellationToken::new();
        let denial = Arc::new(StdMutex::new(None));
        let stop = CancellationToken::new();
        let changes = enforcer.store.subscribe_changes();
        let task = tokio::spawn(run_monitor(
            enforcer.clone(),
            agent_type,
            capability,
            runtime_verified,
            cancel_target.clone(),
            revoked.clone(),
            Arc::clone(&denial),
            stop.clone(),
            changes,
        ));
        Self {
            enforcer,
            agent_type,
            capability,
            runtime_verified,
            cancel_target,
            revoked,
            denial,
            stop,
            task,
        }
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.revoked.clone()
    }

    pub fn error_if_revoked(&self) -> Result<(), AppCommandError> {
        if !self.revoked.is_cancelled() {
            return Ok(());
        }
        Err(self
            .denial
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .unwrap_or_else(|| denied(DenialCode::RemotePolicyDenied)))
    }

    pub async fn require_current(&self) -> Result<(), AppCommandError> {
        self.error_if_revoked()?;
        let result = require_capability(
            &self.enforcer,
            self.agent_type,
            self.capability,
            self.runtime_verified,
        )
        .await;
        if let Err(error) = result {
            record_revocation(
                self.capability,
                error.clone(),
                &self.cancel_target,
                &self.revoked,
                &self.denial,
            );
            return Err(error);
        }
        self.error_if_revoked()
    }

    pub async fn run_until_revoked<T>(
        &self,
        future: impl Future<Output = T>,
    ) -> Result<T, AppCommandError> {
        tokio::select! {
            biased;
            _ = self.revoked.cancelled() => {
                self.error_if_revoked()?;
                Err(denied(DenialCode::RemotePolicyDenied))
            }
            output = future => {
                self.require_current().await?;
                Ok(output)
            }
        }
    }
}

impl Drop for CapabilityRevocationMonitor {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}

async fn run_monitor(
    enforcer: CapabilityEnforcer,
    agent_type: Option<AgentType>,
    capability: Capability,
    runtime_verified: bool,
    cancel_target: Option<CancellationToken>,
    revoked: CancellationToken,
    denial: Arc<StdMutex<Option<AppCommandError>>>,
    stop: CancellationToken,
    mut changes: tokio::sync::watch::Receiver<u64>,
) {
    loop {
        tokio::select! {
            _ = stop.cancelled() => return,
            result = changes.changed() => {
                if result.is_err() {
                    return;
                }
            }
            _ = tokio::time::sleep(REVOCATION_POLL_INTERVAL) => {}
        }
        let result = require_capability(&enforcer, agent_type, capability, runtime_verified).await;
        let Err(error) = result else {
            continue;
        };
        record_revocation(capability, error, &cancel_target, &revoked, &denial);
        return;
    }
}

async fn require_capability(
    enforcer: &CapabilityEnforcer,
    agent_type: Option<AgentType>,
    capability: Capability,
    runtime_verified: bool,
) -> Result<(), AppCommandError> {
    match agent_type {
        Some(agent_type) => {
            enforcer
                .require_existing_agent(agent_type, capability, runtime_verified)
                .await
        }
        None => enforcer.require_client(capability, runtime_verified).await,
    }
}

fn record_revocation(
    capability: Capability,
    error: AppCommandError,
    cancel_target: &Option<CancellationToken>,
    revoked: &CancellationToken,
    denial: &Arc<StdMutex<Option<AppCommandError>>>,
) {
    let first_revocation = !revoked.is_cancelled();
    *denial
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error.clone());
    revoked.cancel();
    if let Some(target) = cancel_target.as_ref() {
        target.cancel();
    }
    if first_revocation {
        tracing::warn!(
            capability = capability.key(),
            denial_code = error.detail.as_deref().unwrap_or("remote_policy_denied"),
            "[capability-policy] Active operation revoked"
        );
    }
}
