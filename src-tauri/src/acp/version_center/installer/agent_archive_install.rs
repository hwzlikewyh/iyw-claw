use std::path::Path;

use sea_orm::DatabaseConnection;

use super::agent_activation::activate_or_defer;
use super::agent_archive::extract_archive;
use super::signature::verify_agent_signature;
use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::binary_cache;
use crate::acp::error::AcpError;
use crate::acp::registry::{self, AgentDistribution};
use crate::acp::version_center::client::AgentPlatformClient;
use crate::acp::version_center::inventory::{self, ReadyAgentInstallation};
use crate::acp::version_center::types::{AgentOffer, DownloadTicket, ResolveAgentRequest};
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

pub(super) struct ResolvedBinaryInstall<'a> {
    pub conn: &'a DatabaseConnection,
    pub paths: &'a AgentStoragePaths,
    pub agent_type: AgentType,
    pub current_version: Option<&'a str>,
    pub offer: &'a AgentOffer,
    pub ticket: &'a DownloadTicket,
    pub archive: &'a Path,
    pub stage: &'a Path,
    pub channel: &'a str,
    pub on_progress: &'a (dyn Fn(&str) + Send + Sync),
    pub defer_while_active: bool,
    pub reason: &'a str,
}

pub(super) async fn install_resolved_archive(
    request: ResolvedBinaryInstall<'_>,
) -> Result<String, AcpError> {
    let bytes = read_verified_archive(&request).await?;
    let executable_name = stage_executable(&request, &bytes)?;
    let confirmed = confirm_offer(&request).await?;
    validate_confirmed_offer(&request, &confirmed)?;
    activate_staged_binary(&request, &executable_name)?;
    record_and_activate(&request, &confirmed).await?;
    Ok(request.offer.version.clone())
}

async fn read_verified_archive(request: &ResolvedBinaryInstall<'_>) -> Result<Vec<u8>, AcpError> {
    let bytes = tokio::fs::read(request.archive)
        .await
        .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
    verify_agent_signature(&bytes, &request.ticket.signature).map_err(app_error)?;
    (request.on_progress)("Agent artifact integrity and signature verified");
    Ok(bytes)
}

fn stage_executable(request: &ResolvedBinaryInstall<'_>, bytes: &[u8]) -> Result<String, AcpError> {
    let executable_name = executable_name(request.agent_type)?;
    let extracted = request.stage.join("extracted");
    extract_archive(bytes, &request.ticket.file_name, &extracted).map_err(app_error)?;
    let source = binary_cache::find_binary_recursive(&extracted, &executable_name)
        .ok_or_else(|| AcpError::DownloadFailed("Agent executable is missing".into()))?;
    std::fs::copy(source, request.stage.join(&executable_name))
        .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
    std::fs::remove_dir_all(&extracted).map_err(|error| {
        AcpError::DownloadFailed(format!("failed to finalize Agent staging: {error}"))
    })?;
    Ok(executable_name)
}

async fn confirm_offer(request: &ResolvedBinaryInstall<'_>) -> Result<AgentOffer, AcpError> {
    AgentPlatformClient::resolve_agent(
        request.conn,
        ResolveAgentRequest {
            registry_id: registry::registry_id_for(request.agent_type),
            current_version: request.current_version.unwrap_or_default(),
            requested_version: Some(&request.offer.version),
            pinned_version: None,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: super::super::capability::RUNTIME,
            target: super::super::capability::current_target(),
            arch: super::super::capability::current_arch(),
            channel: request.channel,
            reason: request.reason,
        },
    )
    .await
    .map_err(app_error)
}

fn validate_confirmed_offer(
    request: &ResolvedBinaryInstall<'_>,
    confirmed: &AgentOffer,
) -> Result<(), AcpError> {
    if confirmed.version_id == request.offer.version_id
        && confirmed.delivery.artifact_id == request.offer.delivery.artifact_id
    {
        return Ok(());
    }
    Err(AcpError::DownloadFailed(
        "Agent offer changed before activation".into(),
    ))
}

fn activate_staged_binary(
    request: &ResolvedBinaryInstall<'_>,
    executable_name: &str,
) -> Result<(), AcpError> {
    let binary = binary_cache::activate_staged_binary(
        request.paths,
        registry::registry_id_for(request.agent_type),
        &request.offer.version,
        executable_name,
        request.stage,
    )?;
    binary
        .is_file()
        .then_some(())
        .ok_or_else(|| AcpError::DownloadFailed("Agent executable activation failed".into()))
}

async fn record_and_activate(
    request: &ResolvedBinaryInstall<'_>,
    confirmed: &AgentOffer,
) -> Result<(), AcpError> {
    inventory::record_agent_ready(
        request.conn,
        ReadyAgentInstallation {
            agent_type: request.agent_type,
            registry_id: &request.offer.registry_id,
            version: &request.offer.version,
            delivery_kind: "binary",
            artifact_id: request.offer.delivery.artifact_id.as_deref(),
            source_key: None,
            expected_sha256: Some(&request.ticket.sha256),
        },
    )
    .await?;
    activate_or_defer(
        request.conn,
        request.agent_type,
        &request.offer.version,
        &confirmed.effective_update_policy,
        confirmed.revision,
        request.defer_while_active,
    )
    .await
}

fn executable_name(agent_type: AgentType) -> Result<String, AcpError> {
    let cmd = match registry::get_agent_meta(agent_type).distribution {
        AgentDistribution::Binary { cmd, .. } => cmd,
        _ => return Err(AcpError::protocol("Agent is not binary-based")),
    };
    Ok(if cfg!(windows) {
        format!("{cmd}.exe")
    } else {
        cmd.to_string()
    })
}

fn app_error(error: AppCommandError) -> AcpError {
    let detail = error
        .detail
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or(error.message);
    AcpError::DownloadFailed(detail)
}
