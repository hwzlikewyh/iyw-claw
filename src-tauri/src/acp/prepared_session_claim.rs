use super::{ConnectionManager, Entry};
use crate::acp::error::AcpError;
use crate::acp::prepared_session::PrepareSessionRequest;
use crate::acp::types::ConnectionStatus;
use std::sync::atomic::Ordering;
use std::time::Instant;

impl ConnectionManager {
    pub(crate) async fn claim_prepared_session(
        &self,
        request: &PrepareSessionRequest,
        owner: &str,
    ) -> Result<Option<String>, AcpError> {
        let entry = self
            .prepared_sessions
            .lock()
            .await
            .entries
            .values()
            .filter(|entry| entry.same_target(request, owner))
            .max_by_key(|entry| (entry.matches(request, owner), entry.created))
            .cloned();
        let Some(entry) = entry else {
            return Ok(None);
        };
        let started = Instant::now();
        let Some(expected) = entry.ready_for(request, owner).await? else {
            return Ok(None);
        };
        let _operation = self.acquire_operation_read().await?;
        let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
        let current = match self.current_prepared_fingerprint(request).await {
            Ok(fingerprint) => fingerprint,
            Err(error) => {
                entry.cancel.cancel();
                return Err(error);
            }
        };
        if current != expected || !self.prepared_connection_is_live(&entry).await {
            entry.cancel.cancel();
            tracing::info!(
                preparation_id = entry.id,
                "[ACP][prepare] ready state or configuration changed"
            );
            entry.wait_retired().await?;
            return Ok(None);
        }
        let claimed = self.activate_prepared_session(&entry).await;
        if !claimed && !entry.is_claimed() {
            entry.wait_retired().await?;
        }
        tracing::info!(
            preparation_id = entry.id,
            claimed,
            duration_ms = started.elapsed().as_millis(),
            "[ACP][prepare] claim completed"
        );
        Ok(claimed.then(|| entry.id.clone()))
    }

    async fn current_prepared_fingerprint(
        &self,
        request: &PrepareSessionRequest,
    ) -> Result<String, AcpError> {
        let db = self.preparation_db()?;
        if let Some(id) = request.conversation_id {
            let row = crate::db::service::conversation_service::get_by_id(&db.conn, id)
                .await
                .map_err(|error| AcpError::protocol(error.to_string()))?;
            let folder =
                crate::db::service::folder_service::get_folder_by_id(&db.conn, row.folder_id)
                    .await
                    .map_err(|error| AcpError::protocol(error.to_string()))?;
            if row.agent_type != request.agent_type
                || row.external_id != request.session_id
                || folder.as_ref().map(|folder| folder.path.as_str())
                    != request.working_dir.as_deref()
            {
                return Err(AcpError::protocol("Prepared conversation target changed"));
            }
        }
        let mut environment = crate::commands::acp::prepared_session_runtime_env(
            &db,
            request,
            self.preparation_data_dir()?,
        )
        .await?;
        self.require_agent_launch_policy(request.agent_type, true)
            .await?;
        self.prepared_environment_fingerprint(request, &mut environment)
            .await
    }

    async fn activate_prepared_session(&self, entry: &Entry) -> bool {
        let mut pool = self.prepared_sessions.lock().await;
        if !pool.entries.contains_key(&entry.id) || entry.cancel.is_cancelled() {
            return false;
        }
        entry.claimed.store(true, Ordering::Release);
        entry.progress.send_modify(|_| {});
        pool.entries.remove(&entry.id);
        drop(pool);
        self.touch(&entry.id).await;
        if let Some((state, emitter)) = self.get_state_and_emitter(&entry.id).await {
            let session_id = state.read().await.external_id.clone();
            if let Some(session_id) = session_id {
                crate::web::event_bridge::emit_with_state(
                    &state,
                    &emitter,
                    crate::acp::types::AcpEvent::SessionStarted { session_id },
                )
                .await;
            }
        }
        true
    }

    async fn prepared_connection_is_live(&self, entry: &Entry) -> bool {
        let state = self
            .connections
            .lock()
            .await
            .get(&entry.id)
            .map(|connection| connection.state.clone());
        let Some(state) = state else {
            return false;
        };
        let state = state.read().await;
        state.selectors_ready && state.status == ConnectionStatus::Connected
    }
}
