use std::path::{Component, Path, PathBuf};

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
    let entrypoint = stage_artifact(&request, &bytes)?;
    let confirmed = confirm_offer(&request).await?;
    validate_confirmed_offer(&request, &confirmed)?;
    activate_staged_binary(&request, &entrypoint)?;
    record_and_activate(&request, &confirmed).await?;
    Ok(request.offer.version.clone())
}

async fn read_verified_archive(request: &ResolvedBinaryInstall<'_>) -> Result<Vec<u8>, AcpError> {
    let bytes = tokio::fs::read(request.archive)
        .await
        .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
    verify_agent_signature(&bytes, &request.ticket.signature).map_err(app_error)?;
    let progress = if request.ticket.signature.trim().is_empty() {
        "Agent artifact integrity verified"
    } else {
        "Agent artifact integrity and signature verified"
    };
    (request.on_progress)(progress);
    Ok(bytes)
}

fn stage_artifact(request: &ResolvedBinaryInstall<'_>, bytes: &[u8]) -> Result<PathBuf, AcpError> {
    let entrypoint = trusted_entrypoint(request.agent_type)?;
    extract_archive(bytes, &request.ticket.file_name, request.stage).map_err(app_error)?;
    request
        .stage
        .join(&entrypoint)
        .is_file()
        .then_some(entrypoint)
        .ok_or_else(|| AcpError::DownloadFailed("Agent executable is missing".into()))
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
    entrypoint: &Path,
) -> Result<(), AcpError> {
    let binary = binary_cache::activate_staged_binary(
        request.paths,
        registry::registry_id_for(request.agent_type),
        &request.offer.version,
        entrypoint,
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

fn trusted_entrypoint(agent_type: AgentType) -> Result<PathBuf, AcpError> {
    let cmd = match registry::get_agent_meta(agent_type).distribution {
        AgentDistribution::Binary { cmd, .. } => cmd,
        _ => return Err(AcpError::protocol("Agent is not binary-based")),
    };
    let entrypoint = Path::new(cmd);
    let valid = !entrypoint.as_os_str().is_empty()
        && !entrypoint.is_absolute()
        && entrypoint
            .components()
            .all(|component| matches!(component, Component::CurDir | Component::Normal(_)));
    valid
        .then(|| entrypoint.to_path_buf())
        .ok_or_else(|| AcpError::DownloadFailed("Agent executable path is invalid".into()))
}

fn app_error(error: AppCommandError) -> AcpError {
    let detail = error
        .detail
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or(error.message);
    AcpError::DownloadFailed(detail)
}
