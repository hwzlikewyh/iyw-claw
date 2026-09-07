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
        let results = tokio::join!(
            self.prewarm_agent_runtime(AgentType::Codex),
            self.prewarm_agent_runtime(AgentType::ClaudeCode),
        );
        for (agent, result) in [("星河", results.0), ("远山", results.1)] {
            match result {
                Ok(ready) => tracing::info!(agent, ready, "[ACP][startup] runtime prewarm settled"),
                Err(error) => {
                    tracing::warn!(agent, %error, "[ACP][startup] runtime prewarm deferred")
                }
            }
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
        let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
        let started = Instant::now();
        let Some(request) = self.prepare_runtime_prewarm(agent_type).await? else {
            return Ok(false);
        };
        if memory_is_tight() {
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
            .find_connection_for_reuse(agent_type, Some(&target.cwd), target.session_id.as_deref())
            .await
            .is_some()
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
}

fn memory_is_tight() -> bool {
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
    if let Some(tab) = tabs.first() {
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
            .filter(|row| row.folder_id == tab.folder_id && row.agent_type == agent.to_string())
            .and_then(|row| row.external_id),
        None => None,
    };
    Ok(Some(RuntimePrewarmTarget {
        cwd: PathBuf::from(folder.path),
        session_id,
    }))
}
