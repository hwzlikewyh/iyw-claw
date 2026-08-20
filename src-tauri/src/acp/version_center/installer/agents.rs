use std::path::PathBuf;

use sea_orm::DatabaseConnection;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::error::AcpError;
use crate::acp::registry;
use crate::acp::version_center::capability::{self, RUNTIME};
use crate::acp::version_center::client::AgentPlatformClient;
use crate::acp::version_center::types::{AgentOffer, ResolveAgentRequest};
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

use super::agent_archive_install::{install_resolved_archive, ResolvedBinaryInstall};
use super::agent_download::download_archive;
use super::resumable::cleanup_resumable_files;

pub(crate) struct ManagedBinaryAgentRequest<'a, F: Fn(&str) + Send + Sync> {
    pub conn: &'a DatabaseConnection,
    pub paths: &'a AgentStoragePaths,
    pub agent_type: AgentType,
    pub requested_version: &'a str,
    pub channel: &'a str,
    pub on_progress: F,
    pub defer_while_active: bool,
    pub reason: &'a str,
}

struct BinaryInstallContext<'a, F: Fn(&str) + Send + Sync> {
    conn: &'a DatabaseConnection,
    paths: &'a AgentStoragePaths,
    agent_type: AgentType,
    current_version: Option<&'a str>,
    channel: &'a str,
    on_progress: &'a F,
    defer_while_active: bool,
    reason: &'a str,
}

struct InstallWorkspace {
    stage: PathBuf,
    archive: PathBuf,
}

pub(crate) async fn install_managed_binary_agent<F>(
    request: ManagedBinaryAgentRequest<'_, F>,
) -> Result<String, AcpError>
where
    F: Fn(&str) + Send + Sync,
{
    let setting = crate::db::service::agent_setting_service::get_by_agent_type(
        request.conn,
        request.agent_type,
    )
    .await
    .map_err(|error| AcpError::protocol(error.to_string()))?;
    let context = BinaryInstallContext {
        conn: request.conn,
        paths: request.paths,
        agent_type: request.agent_type,
        current_version: setting
            .as_ref()
            .and_then(|item| item.installed_version.as_deref()),
        channel: request.channel,
        on_progress: &request.on_progress,
        defer_while_active: request.defer_while_active,
        reason: request.reason,
    };
    let offer = resolve_offer(&context, request.requested_version)
        .await
        .map_err(app_error)?;
    let workspace = prepare_workspace(&context).await?;
    let result = install_resolved_binary(&context, &offer, &workspace).await;
    cleanup_workspace(&workspace).await;
    result
}

async fn prepare_workspace(
    context: &BinaryInstallContext<'_, impl Fn(&str) + Send + Sync>,
) -> Result<InstallWorkspace, AcpError> {
    let operation = uuid::Uuid::new_v4();
    let stage = context.paths.staging_dir().join(format!(
        "agent-{}-{operation}",
        registry::registry_id_for(context.agent_type)
    ));
    let archive = context
        .paths
        .downloads_dir()
        .join(format!("agent-{operation}.archive"));
    tokio::fs::create_dir_all(context.paths.staging_dir())
        .await
        .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
    tokio::fs::create_dir_all(context.paths.downloads_dir())
        .await
        .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
    tokio::fs::create_dir(&stage)
        .await
        .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
    Ok(InstallWorkspace { stage, archive })
}

async fn install_resolved_binary<F: Fn(&str) + Send + Sync>(
    context: &BinaryInstallContext<'_, F>,
    offer: &AgentOffer,
    workspace: &InstallWorkspace,
) -> Result<String, AcpError> {
    let result = match download_archive(
        context.conn,
        offer,
        context.current_version,
        context.channel,
        &workspace.archive,
        context.on_progress,
        context.reason == "manual",
    )
    .await
    {
        Ok(ticket) => {
            install_resolved_archive(ResolvedBinaryInstall {
                conn: context.conn,
                paths: context.paths,
                agent_type: context.agent_type,
                current_version: context.current_version,
                offer,
                ticket: &ticket,
                archive: &workspace.archive,
                stage: &workspace.stage,
                channel: context.channel,
                on_progress: context.on_progress,
                defer_while_active: context.defer_while_active,
                reason: context.reason,
            })
            .await
        }
        Err(error) => Err(error.into_error()),
    };
    result
}

async fn cleanup_workspace(workspace: &InstallWorkspace) {
    let _ = tokio::fs::remove_file(&workspace.archive).await;
    cleanup_resumable_files(&workspace.archive).await;
    let _ = tokio::fs::remove_dir_all(&workspace.stage).await;
}

async fn resolve_offer(
    context: &BinaryInstallContext<'_, impl Fn(&str) + Send + Sync>,
    requested_version: &str,
) -> Result<AgentOffer, AppCommandError> {
    AgentPlatformClient::resolve_agent(
        context.conn,
        ResolveAgentRequest {
            registry_id: registry::registry_id_for(context.agent_type),
            current_version: context.current_version.unwrap_or_default(),
            requested_version: Some(requested_version),
            pinned_version: None,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel: context.channel,
            reason: context.reason,
        },
    )
    .await
}

fn app_error(error: AppCommandError) -> AcpError {
    let detail = error
        .detail
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or(error.message);
    AcpError::DownloadFailed(detail)
}
