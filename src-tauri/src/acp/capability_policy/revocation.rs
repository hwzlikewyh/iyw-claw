use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::{Capability, CapabilityEnforcer};
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

pub struct CapabilityRevocationMonitor {
    enforcer: CapabilityEnforcer,
    agent_type: Option<AgentType>,
    capability: Capability,
    runtime_verified: bool,
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
        let task = tokio::spawn(run_monitor(
            enforcer.clone(),
            agent_type,
            capability,
            runtime_verified,
            cancel_target.clone(),
            revoked.clone(),
            Arc::clone(&denial),
            stop.clone(),
            enforcer.store.subscribe_changes(),
        ));
        Self {
            enforcer,
            agent_type,
            capability,
            runtime_verified,
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
            .unwrap_or_else(operation_cancelled))
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
        if let Err(error) = &result {
            if !is_immediate_local_denial(error) {
                tracing::info!(
                    capability = self.capability.key(),
                    denial_code = error.detail.as_deref().unwrap_or("remote_policy_denied"),
                    "[capability-policy] Remote denial deferred for active operation lease"
                );
                return Ok(());
            }
            tracing::warn!(
                capability = self.capability.key(),
                denial_code = error.detail.as_deref().unwrap_or("capability_denied"),
                "[capability-policy] New operation denied at capability boundary"
            );
        }
        result
    }

    pub async fn run_until_revoked<T>(
        &self,
        future: impl Future<Output = T>,
    ) -> Result<T, AppCommandError> {
        self.require_current().await?;
        tokio::select! {
            output = future => {
                self.require_current().await?;
                Ok(output)
            }
            _ = self.revoked.cancelled() => self.error_if_revoked().map(|_| unreachable!()),
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
        let error = require_capability(&enforcer, agent_type, capability, runtime_verified).await;
        if let Err(error) = error {
            if is_immediate_local_denial(&error) {
                record_local_revocation(capability, error, &cancel_target, &revoked, &denial);
                return;
            }
        }
        tokio::select! {
            _ = stop.cancelled() => return,
            result = changes.changed() => {
                if result.is_err() {
                    return;
                }
            }
        }
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

fn is_immediate_local_denial(error: &AppCommandError) -> bool {
    matches!(
        error.detail.as_deref(),
        Some(
            "compiled_support_disabled"
                | "local_preference_disabled"
                | "runtime_unverified"
                | "subject_capability_mismatch"
        )
    )
}

fn record_local_revocation(
    capability: Capability,
    error: AppCommandError,
    cancel_target: &Option<CancellationToken>,
    revoked: &CancellationToken,
    denial: &Arc<StdMutex<Option<AppCommandError>>>,
) {
    *denial
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error.clone());
    revoked.cancel();
    if let Some(target) = cancel_target {
        target.cancel();
    }
    tracing::warn!(
        capability = capability.key(),
        denial_code = error.detail.as_deref().unwrap_or("local_capability_denied"),
        "[capability-policy] Active operation stopped by local capability state"
    );
}

fn operation_cancelled() -> AppCommandError {
    AppCommandError::permission_denied("Operation lifecycle ended")
        .with_detail("operation_cancelled")
}
