use std::path::Path;
use std::sync::OnceLock;

use sea_orm::DatabaseConnection;
use serde::Serialize;
use tokio::sync::Mutex;

use super::archive::{extract_tool_zip, locate_payload, probe_payload};
use super::download::{download_archive, validate_ticket};
use super::runtime::{
    read_current_pointer, restore_current_pointer, runtime_dir, staging_dir, write_current_pointer,
};
use super::signature::verify_tool_signature;
use crate::acp::version_center::capability::{self, RUNTIME};
use crate::acp::version_center::client::AgentPlatformClient;
use crate::acp::version_center::inventory::{self, ReadyToolInstallation, ORIGIN_MANAGED};
use crate::acp::version_center::types::{DownloadRequest, ResolveToolRequest, ToolOffer};
use crate::app_error::AppCommandError;

fn install_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedToolInstallResult {
    pub tool_id: String,
    pub version: String,
    pub catalog_revision: u64,
}

pub async fn install_managed_tool(
    conn: &DatabaseConnection,
    data_dir: &Path,
    tool_id: &str,
    requested_version: Option<&str>,
    channel: &str,
) -> Result<ManagedToolInstallResult, AppCommandError> {
    let _guard = install_lock().lock().await;
    validate_request(tool_id, requested_version, channel)?;
    let settings = inventory::list_tool_settings(conn)
        .await
        .map_err(inventory_error)?;
    let setting = settings.iter().find(|item| item.tool_id == tool_id);
    let current_version = setting
        .and_then(|item| item.active_version.as_deref())
        .unwrap_or_default();
    let pinned_version = setting.and_then(|item| item.pinned_version.as_deref());
    let offer = AgentPlatformClient::resolve_tool(
        conn,
        ResolveToolRequest {
            tool_id,
            current_version,
            requested_version,
            pinned_version,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
            reason: "manual",
        },
    )
    .await?;
    install_offer(conn, data_dir, &offer, channel).await
}

async fn install_offer(
    conn: &DatabaseConnection,
    data_dir: &Path,
    offer: &ToolOffer,
    channel: &str,
) -> Result<ManagedToolInstallResult, AppCommandError> {
    let stage = staging_dir(data_dir, &offer.tool_id)?;
    let result = install_offer_inner(conn, data_dir, &stage, offer, channel).await;
    if let Err(error) = tokio::fs::remove_dir_all(&stage).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                tool_id = %offer.tool_id,
                version = %offer.version,
                error = %error,
                "[agent-version-center] managed tool staging cleanup failed"
            );
        }
    }
    result
}

async fn install_offer_inner(
    conn: &DatabaseConnection,
    data_dir: &Path,
    stage: &Path,
    offer: &ToolOffer,
    channel: &str,
) -> Result<ManagedToolInstallResult, AppCommandError> {
    tokio::fs::create_dir_all(stage)
        .await
        .map_err(AppCommandError::io)?;
    let ticket = AgentPlatformClient::download_tool(
        conn,
        DownloadRequest {
            registry_id: None,
            tool_id: Some(&offer.tool_id),
            version_id: &offer.version_id,
            artifact_id: &offer.artifact.id,
            catalog_revision: offer.revision,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
        },
    )
    .await?;
    validate_ticket(
        offer,
        &ticket.url,
        ticket.size,
        &ticket.sha256,
        &ticket.signature,
    )?;
    let archive = stage.join("artifact.zip");
    download_archive(&ticket.url, &archive, ticket.size, &ticket.sha256).await?;
    let bytes = tokio::fs::read(&archive)
        .await
        .map_err(AppCommandError::io)?;
    verify_tool_signature(&bytes, &ticket.signature)?;

    let extract_root = stage.join("payload");
    let tool_id = offer.tool_id.clone();
    let extracted = extract_root.clone();
    tokio::task::spawn_blocking(move || extract_tool_zip(&bytes, &extracted, &tool_id))
        .await
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))??;
    let payload = locate_payload(&extract_root, &offer.tool_id)?;
    probe_payload(&payload, &offer.tool_id, &offer.version).await?;
    let confirmed = confirm_offer(conn, offer, channel).await?;

    let final_dir = runtime_dir(data_dir, &offer.tool_id, &offer.version)?;
    if final_dir.exists() {
        return Err(AppCommandError::invalid_input(
            "Managed tool version is already installed; switch to it instead",
        ));
    }
    let parent = final_dir.parent().ok_or_else(|| {
        AppCommandError::configuration_invalid("Managed tool runtime path is invalid")
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(AppCommandError::io)?;
    tokio::fs::rename(&payload, &final_dir)
        .await
        .map_err(AppCommandError::io)?;

    inventory::record_tool_ready(
        conn,
        ReadyToolInstallation {
            tool_id: &offer.tool_id,
            version: &offer.version,
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            origin: ORIGIN_MANAGED,
            artifact_id: Some(&offer.artifact.id),
            expected_sha256: Some(&offer.artifact.sha256),
        },
    )
    .await
    .map_err(inventory_error)?;

    let previous_pointer = read_current_pointer(data_dir, &offer.tool_id).await?;
    write_current_pointer(data_dir, &offer.tool_id, &offer.version).await?;
    if let Err(error) = inventory::activate_tool(
        conn,
        &offer.tool_id,
        &offer.version,
        &confirmed.effective_update_policy,
        confirmed.revision,
    )
    .await
    {
        restore_current_pointer(data_dir, &offer.tool_id, previous_pointer).await?;
        return Err(inventory_error(error));
    }
    tracing::info!(
        tool_id = %offer.tool_id,
        version = %offer.version,
        revision = confirmed.revision,
        "[agent-version-center] managed tool installed and activated"
    );
    Ok(ManagedToolInstallResult {
        tool_id: offer.tool_id.clone(),
        version: offer.version.clone(),
        catalog_revision: confirmed.revision,
    })
}

async fn confirm_offer(
    conn: &DatabaseConnection,
    offer: &ToolOffer,
    channel: &str,
) -> Result<ToolOffer, AppCommandError> {
    let confirmed = AgentPlatformClient::resolve_tool(
        conn,
        ResolveToolRequest {
            tool_id: &offer.tool_id,
            current_version: "",
            requested_version: Some(&offer.version),
            pinned_version: None,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
            reason: "recovery",
        },
    )
    .await?;
    if confirmed.version_id != offer.version_id
        || confirmed.version != offer.version
        || confirmed.artifact.id != offer.artifact.id
        || !confirmed
            .artifact
            .sha256
            .eq_ignore_ascii_case(&offer.artifact.sha256)
    {
        return Err(AppCommandError::invalid_input(
            "Managed tool offer changed before activation",
        ));
    }
    Ok(confirmed)
}

fn validate_request(
    tool_id: &str,
    requested_version: Option<&str>,
    channel: &str,
) -> Result<(), AppCommandError> {
    if !capability::known_tool(tool_id) || !matches!(channel, "stable" | "beta") {
        return Err(AppCommandError::invalid_input(
            "Invalid managed tool request",
        ));
    }
    if let Some(version) = requested_version.filter(|value| !value.trim().is_empty()) {
        semver::Version::parse(version.trim())
            .map_err(|_| AppCommandError::invalid_input("Invalid managed tool version"))?;
    }
    Ok(())
}

fn inventory_error(error: crate::acp::error::AcpError) -> AppCommandError {
    AppCommandError::task_execution_failed(error.to_string())
}
