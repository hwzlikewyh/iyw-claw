use std::time::Duration;

use crate::acp::agent_storage_work::has_active_agent_storage_work;
use crate::acp::error::AcpError;
use crate::acp::manager::ConnectionManager;
use crate::acp::registry::{self, AgentDistribution};
use crate::commands::acp::{
    acp_download_agent_binary_core, acp_list_agents_core, acp_prepare_npx_agent_core,
};
use crate::db::AppDatabase;
use crate::models::agent::AgentType;
use crate::web::event_bridge::EventEmitter;

pub const AGENT_AUTO_UPDATE_INITIAL_DELAY_SECS: u64 = 300;
pub const AGENT_AUTO_UPDATE_INTERVAL_SECS: u64 = 1_800;
pub const AGENT_AUTO_UPDATE_IDLE_SECS: u64 = 300;

struct AutoUpdateCandidate {
    agent_type: AgentType,
    installed_version: String,
    registry_version: String,
}

pub async fn agent_auto_update_task(
    manager: ConnectionManager,
    db: AppDatabase,
    emitter: EventEmitter,
) {
    tokio::time::sleep(Duration::from_secs(AGENT_AUTO_UPDATE_INITIAL_DELAY_SECS)).await;
    loop {
        run_auto_update_pass(&manager, &db, &emitter).await;
        tokio::time::sleep(Duration::from_secs(AGENT_AUTO_UPDATE_INTERVAL_SECS)).await;
    }
}

async fn run_auto_update_pass(
    manager: &ConnectionManager,
    db: &AppDatabase,
    emitter: &EventEmitter,
) {
    if !is_update_window_open(manager).await {
        return;
    }
    let agents = match acp_list_agents_core(db).await {
        Ok(agents) => agents,
        Err(error) => {
            tracing::warn!(%error, "[ACP] automatic Agent SDK inventory failed");
            return;
        }
    };
    let candidates = agents.into_iter().filter_map(|agent| {
        let installed = agent.installed_version?;
        let registry = agent.registry_version?;
        (agent.available && !versions_match(&installed, &registry)).then_some(AutoUpdateCandidate {
            agent_type: agent.agent_type,
            installed_version: installed,
            registry_version: registry,
        })
    });
    for candidate in candidates {
        if !is_update_window_open(manager).await {
            tracing::info!(
                "[ACP] automatic Agent SDK update paused because Agent activity resumed"
            );
            break;
        }
        update_candidate(candidate, db, emitter).await;
    }
}

async fn is_update_window_open(manager: &ConnectionManager) -> bool {
    !has_active_agent_storage_work()
        && manager
            .is_globally_idle(Duration::from_secs(AGENT_AUTO_UPDATE_IDLE_SECS))
            .await
}

async fn update_candidate(
    candidate: AutoUpdateCandidate,
    db: &AppDatabase,
    emitter: &EventEmitter,
) {
    tracing::info!(
        agent = %candidate.agent_type,
        installed_version = %candidate.installed_version,
        registry_version = %candidate.registry_version,
        "[ACP] automatic Agent SDK update started"
    );
    let result = install_registry_version(&candidate, db, emitter).await;
    match result {
        Ok(()) => tracing::info!(
            agent = %candidate.agent_type,
            version = %candidate.registry_version,
            "[ACP] automatic Agent SDK update completed"
        ),
        Err(error) => tracing::warn!(
            agent = %candidate.agent_type,
            version = %candidate.registry_version,
            %error,
            "[ACP] automatic Agent SDK update failed"
        ),
    }
}

async fn install_registry_version(
    candidate: &AutoUpdateCandidate,
    db: &AppDatabase,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let task_id = format!(
        "agent-auto-update-{}-{}",
        registry::registry_id_for(candidate.agent_type),
        uuid::Uuid::new_v4()
    );
    match registry::get_agent_meta(candidate.agent_type).distribution {
        AgentDistribution::Binary { .. } => {
            acp_download_agent_binary_core(
                candidate.agent_type,
                Some(candidate.registry_version.clone()),
                task_id,
                db,
                emitter,
            )
            .await
        }
        AgentDistribution::Npx { .. } | AgentDistribution::Uvx { .. } => {
            acp_prepare_npx_agent_core(
                candidate.agent_type,
                Some(candidate.registry_version.clone()),
                None,
                false,
                task_id,
                db,
                emitter,
            )
            .await
            .map(|_| ())
        }
    }
}

fn versions_match(installed: &str, registry: &str) -> bool {
    normalize_version(installed).eq_ignore_ascii_case(normalize_version(registry))
}

fn normalize_version(version: &str) -> &str {
    let trimmed = version.trim();
    trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed)
}
