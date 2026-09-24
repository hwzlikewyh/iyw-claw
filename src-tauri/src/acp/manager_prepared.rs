use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::acp::error::AcpError;
use crate::acp::prepared_session::{PrepareSessionRequest, PreparedSessionHandle};
use crate::db::AppDatabase;
use crate::models::AgentType;
use crate::web::event_bridge::EventEmitter;

use super::ConnectionManager;

#[path = "prepared_session_claim.rs"]
mod claim;
#[path = "prepared_session_entry.rs"]
mod entry;
#[path = "prepared_session_fingerprint.rs"]
mod fingerprint;
#[path = "prepared_session_lifecycle.rs"]
mod lifecycle;

pub(super) use entry::Entry;
use entry::{MAX_PREPARED_SESSIONS, PREPARATION_LIFETIME};

#[derive(Default)]
pub(super) struct PreparedSessions {
    pub entries: HashMap<String, Arc<Entry>>,
}

impl PreparedSessions {
    fn has_capacity_for(&self, request: &PrepareSessionRequest) -> bool {
        self.entries.len() < MAX_PREPARED_SESSIONS
            && !request.session_id.as_ref().is_some_and(|session_id| {
                self.entries.values().any(|entry| {
                    entry.request.agent_type == request.agent_type
                        && entry.request.session_id.as_ref() == Some(session_id)
                })
            })
    }
}

impl ConnectionManager {
    pub(crate) async fn prepare_session(
        &self,
        request: PrepareSessionRequest,
        context: (String, EventEmitter),
    ) -> Result<Option<PreparedSessionHandle>, AcpError> {
        if !matches!(request.agent_type, AgentType::Codex | AgentType::ClaudeCode)
            || super::prewarm::memory_is_tight()
            || crate::acp::agent_storage_work::has_active_agent_storage_work()
        {
            return Ok(None);
        }
        let _spawn = self.connection_tasks.begin_spawn().await?;
        let Ok(_budget) = self.speculative_runtime_gate.try_lock() else {
            return Ok(None);
        };
        let (owner, emitter) = context;
        let pool = self.prepared_sessions.lock().await;
        if let Some(entry) = pool
            .entries
            .values()
            .find(|entry| entry.matches(&request, &owner))
        {
            return Ok(Some(entry.handle()));
        }
        drop(pool);
        if self.speculative_runtime_capacity().await? == 0 {
            return Ok(None);
        }
        let mut pool = self.prepared_sessions.lock().await;
        if !pool.has_capacity_for(&request) {
            return Ok(None);
        }
        let entry = Arc::new(self.allocate_preparation(request, (owner, emitter))?);
        pool.entries.insert(entry.id.clone(), entry.clone());
        drop(pool);
        let handle = entry.handle();
        let manager = self.clone_ref();
        let task = tokio::spawn(async move {
            manager.run_preparation(entry).await;
        });
        self.connection_tasks
            .register(format!("prepared:{}", handle.id), task)
            .await;
        Ok(Some(handle))
    }

    fn allocate_preparation(
        &self,
        request: PrepareSessionRequest,
        context: (String, EventEmitter),
    ) -> Result<Entry, AcpError> {
        let (owner, emitter) = context;
        let owns_workspace = request.working_dir.is_none();
        let working_dir = match request.working_dir.as_ref() {
            Some(cwd) if Path::new(cwd).is_absolute() && Path::new(cwd).is_dir() => cwd.clone(),
            Some(_) => {
                return Err(AcpError::protocol(
                    "Session preparation requires an existing absolute workspace",
                ))
            }
            None if request.session_id.is_none() && request.conversation_id.is_none() => {
                crate::commands::conversations::create_chat_dir_core(self.preparation_data_dir()?)
                    .map_err(|error| AcpError::protocol(error.to_string()))?
            }
            None => {
                return Err(AcpError::protocol(
                    "Recovery preparation requires a working directory",
                ))
            }
        };
        Ok(Entry::new(
            request,
            (owner, working_dir, owns_workspace),
            &emitter,
        ))
    }

    pub(crate) async fn cancel_preparation(&self, id: &str, owner: &str) {
        let pool = self.prepared_sessions.lock().await;
        if let Some(entry) = pool.entries.get(id).filter(|entry| entry.owner == owner) {
            entry.cancel.cancel();
        }
    }

    pub(crate) async fn reserve_prepared_workspace(&self, id: &str, owner: &str) -> Option<String> {
        let pool = self.prepared_sessions.lock().await;
        let entry = pool.entries.get(id)?;
        if entry.owner != owner
            || !entry.owns_workspace
            || entry.cancel.is_cancelled()
            || entry.created.elapsed() >= PREPARATION_LIFETIME
        {
            return None;
        }
        entry
            .workspace_reserved
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .ok()?;
        Some(entry.working_dir.clone())
    }

    pub(super) async fn cancel_preparations_by_owner(&self, owner: Option<&str>) {
        for entry in self.prepared_sessions.lock().await.entries.values() {
            if owner.is_none_or(|owner| entry.owner == owner) {
                entry.cancel.cancel();
            }
        }
    }

    pub(super) async fn finish_preparation_shutdown(&self) {
        for (_, entry) in self.prepared_sessions.lock().await.entries.drain() {
            entry
                .progress
                .send_modify(|progress| progress.retired = true);
        }
    }

    fn preparation_data_dir(&self) -> Result<&Path, AcpError> {
        self.version_center_data_dir
            .get()
            .map(|path| path.as_path())
            .ok_or_else(|| AcpError::protocol("Agent data directory unavailable"))
    }

    fn preparation_db(&self) -> Result<AppDatabase, AcpError> {
        self.version_center_db
            .get()
            .map(|conn| AppDatabase { conn: conn.clone() })
            .ok_or_else(|| AcpError::protocol("Agent platform unavailable"))
    }

    pub(super) async fn retire_conflicting_preparations(
        &self,
        target: (AgentType, Option<&str>),
        except: Option<&str>,
    ) -> Result<(), AcpError> {
        let Some(session_id) = target.1 else {
            return Ok(());
        };
        let entries: Vec<_> = self
            .prepared_sessions
            .lock()
            .await
            .entries
            .values()
            .filter(|entry| {
                Some(entry.id.as_str()) != except
                    && entry.request.agent_type == target.0
                    && entry.request.session_id.as_deref() == Some(session_id)
            })
            .cloned()
            .collect();
        for entry in entries {
            entry.cancel.cancel();
            entry.wait_retired().await?;
        }
        Ok(())
    }
}
