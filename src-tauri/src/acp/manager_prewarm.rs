use std::path::PathBuf;
use std::time::Instant;

use crate::acp::connection::{RuntimePrewarmRequest, RuntimePrewarmTarget};
use crate::acp::error::AcpError;
use crate::db::service::{conversation_service, folder_service, tab_service};
use crate::db::AppDatabase;
use crate::models::AgentType;

use super::ConnectionManager;

impl ConnectionManager {
    pub async fn prewarm_primary_agents(&self) {
        let Some(db) = self.version_center_db.get() else { return };
        let tabs = match tab_service::list_all_tabs(db).await {
            Ok(tabs) => tabs,
            Err(error) => {
                tracing::warn!(%error, "[ACP][startup] active tab unavailable for prewarm");
                return;
            }
        };
        let Some(agent) = tabs.iter().find(|tab| tab.is_active).map(|tab| tab.agent_type) else {
            return;
        };
        if !matches!(agent, AgentType::Codex | AgentType::ClaudeCode) { return; }
        match self.prewarm_agent_runtime(agent).await {
            Ok(ready) => tracing::info!(agent = %agent, ready, "[ACP][startup] runtime prewarm settled"),
            Err(error) => tracing::warn!(agent = %agent, %error, "[ACP][startup] runtime prewarm deferred"),
        }
    }

    pub async fn prewarm_codex_runtime(&self) -> Result<bool, AcpError> {
        self.prewarm_agent_runtime(AgentType::Codex).await
    }

    async fn prewarm_agent_runtime(&self, agent_type: AgentType) -> Result<bool, AcpError> {
        let _operation = self.acquire_operation_read().await?;
        if crate::acp::agent_storage_work::has_active_agent_storage_work() || memory_is_tight() {
            return Ok(false);
        }
        let _budget = self.speculative_runtime_gate.lock().await;
        if self.speculative_runtime_capacity().await? == 0 {
            return Ok(false);
        }
        let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
        let started = Instant::now();
        let Some(request) = self.prepare_runtime_prewarm(agent_type).await? else {
            return Ok(false);
        };
        if memory_is_tight()
            || self
                .has_connection_for_prewarm_target(agent_type, &request.target)
                .await
        {
            return Ok(false);
        }
        let ready =
            crate::acp::connection::prewarm_agent_runtime(request, self.runtime_hosts.clone())
                .await?;
        let agent = if agent_type == AgentType::Codex {
            "星河"
        } else {
            "远山"
        };
        tracing::info!(
            agent,
            elapsed_ms = started.elapsed().as_millis(),
            ready,
            "[ACP][startup] primary runtime prepared"
        );
        Ok(ready)
    }
    async fn prepare_runtime_prewarm(
        &self,
        agent_type: AgentType,
    ) -> Result<Option<RuntimePrewarmRequest>, AcpError> {
        let conn = self
            .version_center_db
            .get()
            .ok_or_else(|| AcpError::protocol("Agent platform unavailable"))?;
        let data_dir = self
            .version_center_data_dir
            .get()
            .ok_or_else(|| AcpError::protocol("Agent data directory unavailable"))?;
        let target = prewarm_target(conn, agent_type).await?;
        if self
            .has_connection_for_prewarm_target(agent_type, &target)
            .await
        {
            return Ok(None);
        }
        let db = AppDatabase { conn: conn.clone() };
        let environment = match crate::commands::acp::build_session_runtime_env(
            &db,
            agent_type,
            target.session_id.as_deref(),
            data_dir,
        )
        .await
        {
            Ok(environment) => environment,
            Err(AcpError::SdkNotInstalled(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        crate::commands::acp::verify_agent_installed(agent_type, &environment)?;
        crate::acp::account_credentials::sync_agent_credentials_for_acp(conn, agent_type).await?;
        self.require_agent_launch_policy(agent_type, true).await?;
        Ok(Some(RuntimePrewarmRequest {
            agent_type,
            environment,
            target,
        }))
    }

    async fn has_connection_for_prewarm_target(
        &self,
        agent_type: AgentType,
        target: &RuntimePrewarmTarget,
    ) -> bool {
        let Some(session_id) = target.session_id.as_deref() else {
            return false;
        };
        let states: Vec<_> = self
            .connections
            .lock()
            .await
            .values()
            .filter(|connection| connection.agent_type == agent_type)
            .map(|connection| connection.state.clone())
            .collect();
        for state in states {
            let state = state.read().await;
            let existing = state
                .external_id
                .as_deref()
                .or(state.requested_external_id.as_deref());
            if existing == Some(session_id)
                && state.working_dir.as_ref() == Some(&target.cwd)
                && !matches!(
                    state.status,
                    crate::acp::types::ConnectionStatus::Disconnected
                        | crate::acp::types::ConnectionStatus::Error
                )
            {
                tracing::info!(agent = %agent_type, status = ?state.status,
                    "[ACP][startup] prewarm skipped: target connection already exists");
                return true;
            }
        }
        false
    }
}

pub(super) fn memory_is_tight() -> bool {
    use crate::acp::resource_governor::{system_memory_snapshot, MemoryPressure};
    // 预热门禁只需要可用内存，不枚举整台机器的进程、磁盘和 CPU。
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    matches!(
        system_memory_snapshot(system.total_memory(), system.available_memory()).pressure,
        MemoryPressure::Shrinking | MemoryPressure::Emergency | MemoryPressure::Unknown
    )
}

async fn prewarm_target(
    db: &sea_orm::DatabaseConnection,
    agent: AgentType,
) -> Result<RuntimePrewarmTarget, AcpError> {
    let mut tabs = tab_service::list_all_tabs(db)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    tabs.sort_by_key(|tab| (tab.agent_type != agent, !tab.is_active, tab.position));
    for tab in tabs.iter().filter(|tab| tab.agent_type == agent) {
        if let Some(target) = target_from_tab(db, tab, agent).await? {
            return Ok(target);
        }
    }
    let recent = folder_service::list_open_folders(db)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    let cwd = recent
        .first()
        .map(|folder| PathBuf::from(&folder.path))
        .or_else(|| {
            crate::acp::agent_storage::AgentStoragePaths::active()
                .map(|paths| paths.root().to_path_buf())
        })
        .ok_or_else(|| AcpError::protocol("Agent prewarm workspace unavailable"))?;
    Ok(RuntimePrewarmTarget {
        cwd,
        session_id: None,
    })
}

async fn target_from_tab(
    db: &sea_orm::DatabaseConnection,
    tab: &crate::models::OpenedTab,
    agent: AgentType,
) -> Result<Option<RuntimePrewarmTarget>, AcpError> {
    let Some(folder) = folder_service::get_folder_by_id(db, tab.folder_id)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?
    else {
        return Ok(None);
    };
    let session_id = match tab.conversation_id.filter(|_| tab.agent_type == agent) {
        Some(id) => conversation_service::get_by_id(db, id)
            .await
            .ok()
            .filter(|row| row.folder_id == tab.folder_id && row.agent_type == agent)
            .and_then(|row| row.external_id),
        None => None,
    };
    Ok(Some(RuntimePrewarmTarget {
        cwd: PathBuf::from(folder.path),
        session_id,
    }))
}
