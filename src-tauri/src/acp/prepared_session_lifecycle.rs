use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;

use super::{ConnectionManager, Entry, PREPARATION_LIFETIME};
use crate::acp::error::AcpError;
use crate::acp::startup_trace::StartupTrace;
use crate::acp::types::ConnectionStatus;

impl ConnectionManager {
    pub(super) async fn run_preparation(&self, entry: Arc<Entry>) {
        tracing::info!(preparation_id = entry.id, agent = %entry.request.agent_type,
            resumed = entry.request.session_id.is_some(), "[ACP][prepare] started");
        let result = tokio::select! {
            _ = entry.cancel.cancelled() => Err(AcpError::protocol("Session preparation cancelled")),
            result = tokio::time::timeout(PREPARATION_LIFETIME, self.initialize_preparation(&entry)) =>
                result.unwrap_or_else(|_| Err(AcpError::protocol("Session preparation timed out"))),
        };
        if let Err(error) = result {
            entry.fail(&error);
            tracing::info!(preparation_id = entry.id, %error, "[ACP][prepare] ended before readiness");
        } else {
            entry.progress.send_modify(|progress| progress.ready = true);
            tracing::info!(
                preparation_id = entry.id,
                elapsed_ms = entry.created.elapsed().as_millis(),
                "[ACP][prepare] ready"
            );
            self.wait_for_preparation_retirement(&entry).await;
        }
        let retire = {
            let _pool = self.prepared_sessions.lock().await;
            if entry.is_claimed() {
                false
            } else {
                entry.cancel.cancel();
                true
            }
        };
        if retire {
            self.retire_preparation(&entry).await;
        }
        self.prepared_sessions
            .lock()
            .await
            .entries
            .remove(&entry.id);
        entry
            .progress
            .send_modify(|progress| progress.retired = true);
    }

    async fn initialize_preparation(&self, entry: &Arc<Entry>) -> Result<(), AcpError> {
        let request = &entry.request;
        let environment = self.prepare_session_environment(entry).await?;
        let trace = StartupTrace::new(
            request.agent_type,
            request.session_id.is_some(),
            "preparation",
        );
        let id = self
            .spawn_agent_with_origin_traced(
                request.agent_type,
                Some(entry.working_dir.clone()),
                request.session_id.clone(),
                request.conversation_id,
                environment,
                entry.owner.clone(),
                entry.emitter.clone(),
                request.preferred_mode_id.clone(),
                request.preferred_config_values.clone(),
                false,
                crate::user_memory::UserMemoryOrigin::Root,
                trace,
                Some(entry.clone()),
                None,
                None,
            )
            .await?;
        if id != entry.id {
            return Err(AcpError::protocol(
                "An existing connection owns this session",
            ));
        }
        self.wait_for_prepared_readiness(entry).await
    }

    async fn prepare_session_environment(
        &self,
        entry: &Entry,
    ) -> Result<std::collections::BTreeMap<String, String>, AcpError> {
        let db = self.preparation_db()?;
        let request = &entry.request;
        let mut environment = crate::commands::acp::build_session_runtime_env(
            &db,
            request.agent_type,
            request.session_id.as_deref(),
            self.preparation_data_dir()?,
        )
        .await?;
        crate::acp::account_credentials::sync_agent_credentials_for_acp(
            &db.conn,
            request.agent_type,
        )
        .await?;
        self.require_agent_launch_policy(request.agent_type, true)
            .await?;
        let fingerprint = self
            .prepared_environment_fingerprint(request, &mut environment)
            .await?;
        entry
            .progress
            .send_modify(|progress| progress.fingerprint = Some(fingerprint));
        Ok(environment)
    }

    async fn wait_for_prepared_readiness(&self, entry: &Entry) -> Result<(), AcpError> {
        let (state, cancellation) = self
            .connections
            .lock()
            .await
            .get(&entry.id)
            .map(|connection| (connection.state.clone(), connection.cancellation.clone()))
            .ok_or_else(|| AcpError::ConnectionNotFound(entry.id.clone()))?;
        loop {
            let notified = {
                let state = state.read().await;
                if matches!(
                    state.status,
                    ConnectionStatus::Error | ConnectionStatus::Disconnected
                ) {
                    return Err(AcpError::protocol("Prepared session disconnected"));
                }
                if state.selectors_ready {
                    return Ok(());
                }
                state.selectors_ready_notify.clone().notified_owned()
            };
            tokio::select! {
                _ = entry.cancel.cancelled() => return Err(AcpError::protocol("Session preparation cancelled")),
                _ = cancellation.cancelled() => return Err(AcpError::protocol("Prepared connection closed")),
                _ = notified => {}
            }
        }
    }

    async fn wait_for_preparation_retirement(&self, entry: &Entry) {
        let mut progress = entry.progress.subscribe();
        while !entry.is_claimed() {
            let remaining = PREPARATION_LIFETIME.saturating_sub(entry.created.elapsed());
            tokio::select! {
                _ = entry.cancel.cancelled() => break,
                _ = tokio::time::sleep(remaining) => break,
                _ = progress.changed() => {},
            }
        }
    }

    async fn retire_preparation(&self, entry: &Entry) {
        let cleanup = self
            .connections
            .lock()
            .await
            .get(&entry.id)
            .map(|connection| connection.cleanup_completed.clone());
        if cleanup.is_some() {
            let _ = self.disconnect(&entry.id).await;
        }
        if let Some(cleanup) = cleanup {
            while !cleanup.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
        if entry.owns_workspace && !entry.workspace_reserved.load(Ordering::Acquire) {
            // 仅回收本次分配且尚未交给前端的空目录；原生写入保留给既有 scratch GC。
            if let Err(error) = std::fs::remove_dir(&entry.working_dir) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::debug!(preparation_id = entry.id, error_kind = ?error.kind(),
                        "[ACP][prepare] scratch directory left for managed cleanup");
                }
            }
        }
        tracing::info!(preparation_id = entry.id, "[ACP][prepare] retired");
    }
}
