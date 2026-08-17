use std::path::{Path, PathBuf};
use std::time::Instant;

use sea_orm::DatabaseConnection;

use super::agent_download::{download_archive, AgentDownloadError};
use super::resumable::cleanup_resumable_files;
use super::signature::verify_agent_file_signature;
use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::error::AcpError;
use crate::acp::npm_runtime;
use crate::acp::version_center::client::AgentPlatformClient;
use crate::acp::version_center::runtime_bundle_archive::extract_runtime_bundle;
use crate::acp::version_center::runtime_bundle_manifest::validate_bundle_manifest;
use crate::acp::version_center::runtime_bundle_state;
use crate::acp::version_center::types::{AgentOffer, ResolveAgentRequest};
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

pub(crate) struct RuntimeBundleRequest<'a, F: Fn(&str) + Send + Sync> {
    pub conn: &'a DatabaseConnection,
    pub paths: &'a AgentStoragePaths,
    pub agent_type: AgentType,
    pub offer: &'a AgentOffer,
    pub current_version: Option<&'a str>,
    pub required_commands: &'a [&'a str],
    pub reason: &'a str,
    pub on_progress: F,
}

pub(crate) struct InstalledRuntimeBundle {
    pub artifact_id: String,
    pub sha256: String,
    pub revision: u64,
    pub effective_policy: String,
}

pub(crate) enum RuntimeBundleInstallError {
    Unavailable(AcpError),
    Rejected(AcpError),
}

impl RuntimeBundleInstallError {
    pub(crate) fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }

    pub(crate) fn into_error(self) -> AcpError {
        match self {
            Self::Unavailable(error) | Self::Rejected(error) => error,
        }
    }
}

struct BundleWorkspace {
    archive: PathBuf,
    staging: PathBuf,
}

pub(crate) async fn install_runtime_bundle<F>(
    request: RuntimeBundleRequest<'_, F>,
) -> Result<InstalledRuntimeBundle, RuntimeBundleInstallError>
where
    F: Fn(&str) + Send + Sync,
{
    validate_request(&request)?;
    let workspace = prepare_workspace(&request).await?;
    let started = Instant::now();
    let result = install_in_workspace(&request, &workspace).await;
    cleanup_workspace(&workspace).await;
    tracing::info!(
        registry_id = %request.offer.registry_id,
        version = %request.offer.version,
        delivery_kind = %request.offer.delivery.kind,
        target = %request.offer.delivery.target,
        arch = %request.offer.delivery.arch,
        elapsed_ms = started.elapsed().as_millis(),
        succeeded = result.is_ok(),
        "[agent-runtime-bundle] install finished"
    );
    result
}

fn validate_request<F: Fn(&str) + Send + Sync>(
    request: &RuntimeBundleRequest<'_, F>,
) -> Result<(), RuntimeBundleInstallError> {
    if !matches!(request.offer.delivery.kind.as_str(), "npm" | "uvx") {
        return Err(rejected("Agent runtime bundle kind is unsupported"));
    }
    request
        .offer
        .delivery
        .artifact_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|_| ())
        .ok_or_else(|| rejected("Agent runtime bundle offer has no artifact"))
}

async fn prepare_workspace<F: Fn(&str) + Send + Sync>(
    request: &RuntimeBundleRequest<'_, F>,
) -> Result<BundleWorkspace, RuntimeBundleInstallError> {
    let operation = uuid::Uuid::new_v4();
    let archive = request
        .paths
        .downloads_dir()
        .join(format!("agent-runtime-{operation}.archive"));
    let staging = request.paths.staging_dir().join(format!(
        "agent-runtime-{}-{operation}",
        request.offer.registry_id
    ));
    tokio::fs::create_dir_all(request.paths.downloads_dir())
        .await
        .map_err(rejected_io)?;
    tokio::fs::create_dir_all(request.paths.staging_dir())
        .await
        .map_err(rejected_io)?;
    Ok(BundleWorkspace { archive, staging })
}

async fn install_in_workspace<F: Fn(&str) + Send + Sync>(
    request: &RuntimeBundleRequest<'_, F>,
    workspace: &BundleWorkspace,
) -> Result<InstalledRuntimeBundle, RuntimeBundleInstallError> {
    (request.on_progress)("bundle_resolved");
    let ticket = download_archive(
        request.conn,
        request.offer,
        request.current_version,
        &request.offer.channel,
        &workspace.archive,
        &request.on_progress,
        request.reason == "manual",
    )
    .await
    .map_err(download_error)?;
    (request.on_progress)("bundle_downloaded");
    verify_agent_file_signature(&workspace.archive, &ticket.signature).map_err(rejected_app)?;
    extract(&workspace.archive, &ticket.file_name, &workspace.staging).await?;
    let validated =
        validate_bundle_manifest(&workspace.staging, request.offer, request.required_commands)
            .map_err(RuntimeBundleInstallError::Rejected)?;
    (request.on_progress)("bundle_verified");
    let confirmed = confirm_offer(request).await?;
    validate_confirmed(request.offer, &confirmed)?;
    let entrypoint = request
        .required_commands
        .first()
        .and_then(|command| validated.entrypoint(command))
        .ok_or_else(|| rejected("Agent runtime bundle entrypoint is unavailable"))?;
    activate(request, &workspace.staging, entrypoint)?;
    (request.on_progress)("bundle_activated");
    Ok(InstalledRuntimeBundle {
        artifact_id: request
            .offer
            .delivery
            .artifact_id
            .clone()
            .unwrap_or_default(),
        sha256: ticket.sha256,
        revision: confirmed.revision,
        effective_policy: confirmed.effective_update_policy,
    })
}

async fn extract(
    archive: &Path,
    file_name: &str,
    staging: &Path,
) -> Result<(), RuntimeBundleInstallError> {
    let archive = archive.to_path_buf();
    let file_name = file_name.to_string();
    let staging = staging.to_path_buf();
    tokio::task::spawn_blocking(move || extract_runtime_bundle(&archive, &file_name, &staging))
        .await
        .map_err(|error| rejected(format!("Agent runtime extraction task failed: {error}")))?
        .map_err(RuntimeBundleInstallError::Rejected)
}

async fn confirm_offer<F: Fn(&str) + Send + Sync>(
    request: &RuntimeBundleRequest<'_, F>,
) -> Result<AgentOffer, RuntimeBundleInstallError> {
    AgentPlatformClient::resolve_agent(
        request.conn,
        ResolveAgentRequest {
            registry_id: &request.offer.registry_id,
            current_version: request.current_version.unwrap_or_default(),
            requested_version: Some(&request.offer.version),
            pinned_version: None,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: super::super::capability::RUNTIME,
            target: super::super::capability::current_target(),
            arch: super::super::capability::current_arch(),
            channel: &request.offer.channel,
            reason: request.reason,
        },
    )
    .await
    .map_err(|error| classify_app_error(error, request.reason == "manual"))
}

fn validate_confirmed(
    original: &AgentOffer,
    confirmed: &AgentOffer,
) -> Result<(), RuntimeBundleInstallError> {
    let unchanged = confirmed.version_id == original.version_id
        && confirmed.version == original.version
        && confirmed.registry_id == original.registry_id
        && confirmed.delivery.kind == original.delivery.kind
        && confirmed.delivery.target == original.delivery.target
        && confirmed.delivery.arch == original.delivery.arch
        && confirmed.delivery.artifact_id == original.delivery.artifact_id;
    unchanged
        .then_some(())
        .ok_or_else(|| rejected("Agent runtime bundle offer changed before activation"))
}

fn activate<F: Fn(&str) + Send + Sync>(
    request: &RuntimeBundleRequest<'_, F>,
    staging: &Path,
    entrypoint: &Path,
) -> Result<(), RuntimeBundleInstallError> {
    match request.offer.delivery.kind.as_str() {
        "npm" => npm_runtime::activate_private_npm_runtime(
            request.paths,
            request.agent_type,
            &request.offer.version,
            staging,
            request.required_commands,
        )
        .map(|_| ())
        .map_err(RuntimeBundleInstallError::Rejected),
        "uvx" => {
            runtime_bundle_state::activate_uvx_bundle(
                request.paths,
                request.agent_type,
                &request.offer.version,
                staging,
                entrypoint,
            )
            .map_err(RuntimeBundleInstallError::Rejected)?;
            crate::acp::binary_cache::mark_uvx_agent_prepared(
                request.paths,
                request.agent_type,
                &request.offer.version,
            )
            .map_err(RuntimeBundleInstallError::Rejected)
        }
        _ => Err(rejected("Agent runtime bundle kind is unsupported")),
    }
}

async fn cleanup_workspace(workspace: &BundleWorkspace) {
    let _ = tokio::fs::remove_file(&workspace.archive).await;
    cleanup_resumable_files(&workspace.archive).await;
    let _ = tokio::fs::remove_dir_all(&workspace.staging).await;
}

fn download_error(error: AgentDownloadError) -> RuntimeBundleInstallError {
    if error.is_unavailable() {
        RuntimeBundleInstallError::Unavailable(error.into_error())
    } else {
        RuntimeBundleInstallError::Rejected(error.into_error())
    }
}

fn classify_app_error(
    error: AppCommandError,
    allow_policy_missing: bool,
) -> RuntimeBundleInstallError {
    let unavailable = crate::acp::version_center::fallback::allowed(&error, allow_policy_missing);
    let error = app_error(error);
    if unavailable {
        RuntimeBundleInstallError::Unavailable(error)
    } else {
        RuntimeBundleInstallError::Rejected(error)
    }
}

fn rejected(message: impl Into<String>) -> RuntimeBundleInstallError {
    RuntimeBundleInstallError::Rejected(AcpError::DownloadFailed(message.into()))
}

fn rejected_io(error: std::io::Error) -> RuntimeBundleInstallError {
    rejected(error.to_string())
}

fn rejected_app(error: AppCommandError) -> RuntimeBundleInstallError {
    RuntimeBundleInstallError::Rejected(app_error(error))
}

fn app_error(error: AppCommandError) -> AcpError {
    let detail = error
        .detail
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or(error.message);
    AcpError::DownloadFailed(detail)
}
