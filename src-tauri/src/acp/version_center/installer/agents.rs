use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::binary_cache;
use crate::acp::error::AcpError;
use crate::acp::registry::{self, AgentDistribution};
use crate::acp::version_center::capability::{self, RUNTIME};
use crate::acp::version_center::client::AgentPlatformClient;
use crate::acp::version_center::inventory::{self, ReadyAgentInstallation};
use crate::acp::version_center::types::{AgentOffer, ResolveAgentRequest};
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

use super::agent_archive::extract_archive;
use super::agent_download::download_archive;
use super::signature::verify_agent_signature;

pub(crate) async fn install_managed_binary_agent(
    conn: &DatabaseConnection,
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    requested_version: &str,
    channel: &str,
    on_progress: impl Fn(&str),
) -> Result<String, AcpError> {
    let setting = crate::db::service::agent_setting_service::get_by_agent_type(conn, agent_type)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    let offer = resolve_offer(
        conn,
        agent_type,
        setting
            .as_ref()
            .and_then(|item| item.installed_version.as_deref()),
        requested_version,
        channel,
    )
    .await
    .map_err(app_error)?;
    let operation = uuid::Uuid::new_v4();
    let stage = paths.staging_dir().join(format!(
        "agent-{}-{operation}",
        registry::registry_id_for(agent_type)
    ));
    let archive = paths
        .downloads_dir()
        .join(format!("agent-{operation}.archive"));
    tokio::fs::create_dir_all(&stage)
        .await
        .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
    tokio::fs::create_dir_all(paths.downloads_dir())
        .await
        .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
    let result = install_archive(
        conn,
        paths,
        agent_type,
        setting
            .as_ref()
            .and_then(|item| item.installed_version.as_deref()),
        &offer,
        &archive,
        &stage,
        channel,
        on_progress,
    )
    .await;
    let _ = tokio::fs::remove_file(&archive).await;
    let _ = tokio::fs::remove_dir_all(&stage).await;
    result
}

#[allow(clippy::too_many_arguments)]
async fn install_archive(
    conn: &DatabaseConnection,
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    current_version: Option<&str>,
    offer: &AgentOffer,
    archive: &Path,
    stage: &Path,
    channel: &str,
    on_progress: impl Fn(&str),
) -> Result<String, AcpError> {
    let ticket =
        download_archive(conn, offer, current_version, channel, archive, &on_progress).await?;
    let bytes = tokio::fs::read(archive)
        .await
        .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
    verify_agent_signature(&bytes, &ticket.signature).map_err(app_error)?;
    on_progress("Agent artifact integrity and signature verified");

    let cmd = binary_command(agent_type)?;
    let executable_name = if cfg!(windows) {
        format!("{cmd}.exe")
    } else {
        cmd.to_string()
    };
    let extracted = stage.join("extracted");
    extract_archive(&bytes, &ticket.file_name, &extracted).map_err(app_error)?;
    let source = binary_cache::find_binary_recursive(&extracted, &executable_name)
        .ok_or_else(|| AcpError::DownloadFailed("Agent executable is missing".into()))?;
    let staged_binary = stage.join(&executable_name);
    std::fs::copy(source, &staged_binary)
        .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
    std::fs::remove_dir_all(&extracted).map_err(|error| {
        AcpError::DownloadFailed(format!("failed to finalize Agent staging: {error}"))
    })?;
    let confirmed = resolve_offer(conn, agent_type, current_version, &offer.version, channel)
        .await
        .map_err(app_error)?;
    if confirmed.version_id != offer.version_id
        || confirmed.delivery.artifact_id != offer.delivery.artifact_id
    {
        return Err(AcpError::DownloadFailed(
            "Agent offer changed before activation".into(),
        ));
    }
    let binary = binary_cache::activate_staged_binary(
        paths,
        registry::registry_id_for(agent_type),
        &offer.version,
        &executable_name,
        stage,
    )?;
    if !binary.is_file() {
        return Err(AcpError::DownloadFailed(
            "Agent executable activation failed".into(),
        ));
    }
    inventory::record_agent_ready(
        conn,
        ReadyAgentInstallation {
            agent_type,
            registry_id: &offer.registry_id,
            version: &offer.version,
            delivery_kind: "binary",
            artifact_id: offer.delivery.artifact_id.as_deref(),
            source_key: None,
            expected_sha256: Some(&ticket.sha256),
        },
    )
    .await?;
    inventory::activate_agent(
        conn,
        agent_type,
        &offer.version,
        &confirmed.effective_update_policy,
        confirmed.revision,
    )
    .await?;
    Ok(offer.version.clone())
}

async fn resolve_offer(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    current_version: Option<&str>,
    requested_version: &str,
    channel: &str,
) -> Result<AgentOffer, AppCommandError> {
    AgentPlatformClient::resolve_agent(
        conn,
        ResolveAgentRequest {
            registry_id: registry::registry_id_for(agent_type),
            current_version: current_version.unwrap_or_default(),
            requested_version: Some(requested_version),
            pinned_version: None,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
            reason: "manual",
        },
    )
    .await
}

fn binary_command(agent_type: AgentType) -> Result<&'static str, AcpError> {
    match registry::get_agent_meta(agent_type).distribution {
        AgentDistribution::Binary { cmd, .. } => Ok(cmd),
        _ => Err(AcpError::protocol("Agent is not binary-based")),
    }
}

fn app_error(error: AppCommandError) -> AcpError {
    let detail = error
        .detail
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or(error.message);
    AcpError::DownloadFailed(detail)
}
