//! Serialized bootstrap commit for prepared runtime components.

use std::path::Path;

use sea_orm::DatabaseConnection;

use super::activation::quarantine_component;
use super::archive::probe_payload;
use super::bootstrap_component::{ComponentOutcome, PreparedToolComponent};
use super::init::emit_init_event;
use super::manifest::{
    push_pending_activation, upsert_entry, write_manifest, InventoryEntry, InventoryManifest,
    PendingActivation,
};
use super::runtime::{read_current_pointer, restore_current_pointer, write_current_pointer};
use super::state::{write_state, BootstrapState, InitPhase};
use crate::acp::version_center::capability;
use crate::acp::version_center::inventory::{self, ReadyToolInstallation, ORIGIN_MANAGED};
use crate::app_error::AppCommandError;
use crate::web::event_bridge::EventEmitter;

#[allow(clippy::too_many_arguments)]
pub(super) async fn commit_prepared_component(
    conn: &DatabaseConnection,
    data_dir: &Path,
    manifest: &mut InventoryManifest,
    component: &PreparedToolComponent,
    defer_while_active: bool,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<ComponentOutcome, AppCommandError> {
    match component {
        PreparedToolComponent::Keep { version } => Ok(ComponentOutcome {
            version: version.clone(),
            deferred: false,
        }),
        PreparedToolComponent::Deferred { offer } => {
            defer_existing(data_dir, offer).await?;
            Ok(ComponentOutcome {
                version: offer.version.clone(),
                deferred: true,
            })
        }
        PreparedToolComponent::Fresh {
            offer,
            marker,
            payload,
            final_dir,
            ..
        } => {
            install_payload(data_dir, payload, final_dir, marker).await?;
            record_ready(conn, offer).await?;
            commit_installation(
                conn,
                data_dir,
                manifest,
                offer,
                final_dir,
                defer_while_active,
                task_id,
                emitter,
            )
            .await
        }
    }
}

async fn defer_existing(
    data_dir: &Path,
    offer: &crate::acp::version_center::types::ToolOffer,
) -> Result<(), AppCommandError> {
    push_pending_activation(data_dir, pending(offer)).await?;
    tracing::info!(
        tool_id = %offer.tool_id,
        version = %offer.version,
        "[agent-version-center] bootstrap component already installed, activation still deferred"
    );
    Ok(())
}

async fn install_payload(
    data_dir: &Path,
    payload: &Path,
    final_dir: &Path,
    marker: &super::manifest::OwnershipMarker,
) -> Result<(), AppCommandError> {
    if final_dir.exists() {
        quarantine_component(data_dir, final_dir).await?;
    }
    let parent = final_dir.parent().ok_or_else(|| {
        AppCommandError::configuration_invalid("Managed tool runtime path is invalid")
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(AppCommandError::io)?;
    tokio::fs::rename(payload, final_dir)
        .await
        .map_err(AppCommandError::io)?;
    super::manifest::write_marker(final_dir, marker).await
}

async fn record_ready(
    conn: &DatabaseConnection,
    offer: &crate::acp::version_center::types::ToolOffer,
) -> Result<(), AppCommandError> {
    inventory::record_tool_ready(
        conn,
        ReadyToolInstallation {
            tool_id: &offer.tool_id,
            version: &offer.version,
            runtime: capability::RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            origin: ORIGIN_MANAGED,
            artifact_id: Some(&offer.artifact.id),
            expected_sha256: Some(&offer.artifact.sha256),
        },
    )
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))
}

#[allow(clippy::too_many_arguments)]
async fn commit_installation(
    conn: &DatabaseConnection,
    data_dir: &Path,
    manifest: &mut InventoryManifest,
    offer: &crate::acp::version_center::types::ToolOffer,
    final_dir: &Path,
    defer_while_active: bool,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<ComponentOutcome, AppCommandError> {
    if defer_while_active {
        commit_deferred(data_dir, manifest, offer).await?;
        return Ok(ComponentOutcome {
            version: offer.version.clone(),
            deferred: true,
        });
    }
    commit_active(conn, data_dir, manifest, offer, final_dir, task_id, emitter).await
}

async fn commit_deferred(
    data_dir: &Path,
    manifest: &mut InventoryManifest,
    offer: &crate::acp::version_center::types::ToolOffer,
) -> Result<(), AppCommandError> {
    push_pending_activation(data_dir, pending(offer)).await?;
    upsert_entry(manifest, manifest_entry(offer, false));
    write_manifest(data_dir, manifest).await?;
    tracing::info!(
        tool_id = %offer.tool_id,
        version = %offer.version,
        revision = offer.revision,
        "[agent-version-center] bootstrap component installed, activation deferred (active session)"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn commit_active(
    conn: &DatabaseConnection,
    data_dir: &Path,
    manifest: &mut InventoryManifest,
    offer: &crate::acp::version_center::types::ToolOffer,
    final_dir: &Path,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<ComponentOutcome, AppCommandError> {
    emit_init_event(emitter, task_id, "activating", Some(&offer.tool_id), "");
    let previous = read_current_pointer(data_dir, &offer.tool_id).await?;
    write_current_pointer(data_dir, &offer.tool_id, &offer.version).await?;
    activate_inventory(conn, data_dir, offer, final_dir, previous.as_deref()).await?;
    upsert_entry(manifest, manifest_entry(offer, true));
    write_manifest(data_dir, manifest).await?;
    health_check(data_dir, offer, final_dir, previous, task_id, emitter).await?;
    Ok(ComponentOutcome {
        version: offer.version.clone(),
        deferred: false,
    })
}

async fn activate_inventory(
    conn: &DatabaseConnection,
    data_dir: &Path,
    offer: &crate::acp::version_center::types::ToolOffer,
    final_dir: &Path,
    previous: Option<&[u8]>,
) -> Result<(), AppCommandError> {
    if let Err(error) = inventory::activate_tool(
        conn,
        &offer.tool_id,
        &offer.version,
        &offer.effective_update_policy,
        offer.revision,
    )
    .await
    {
        restore_current_pointer(data_dir, &offer.tool_id, previous.map(ToOwned::to_owned)).await?;
        quarantine_component(data_dir, final_dir).await?;
        return Err(AppCommandError::task_execution_failed(error.to_string()));
    }
    Ok(())
}

async fn health_check(
    data_dir: &Path,
    offer: &crate::acp::version_center::types::ToolOffer,
    final_dir: &Path,
    previous: Option<Vec<u8>>,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<(), AppCommandError> {
    emit_init_event(emitter, task_id, "health_check", Some(&offer.tool_id), "");
    if let Err(error) = probe_payload(final_dir, &offer.tool_id, &offer.version).await {
        restore_current_pointer(data_dir, &offer.tool_id, previous).await?;
        quarantine_component(data_dir, final_dir).await?;
        return Err(error);
    }
    Ok(())
}

fn pending(offer: &crate::acp::version_center::types::ToolOffer) -> PendingActivation {
    PendingActivation {
        component_id: offer.tool_id.clone(),
        component_kind: "runtime_tool".to_string(),
        version: offer.version.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        policy: Some(offer.effective_update_policy.clone()),
        revision: Some(offer.revision),
    }
}

fn manifest_entry(
    offer: &crate::acp::version_center::types::ToolOffer,
    active: bool,
) -> InventoryEntry {
    InventoryEntry {
        component_id: offer.tool_id.clone(),
        component_kind: "runtime_tool".to_string(),
        version: offer.version.clone(),
        origin: ORIGIN_MANAGED.to_string(),
        artifact_id: Some(offer.artifact.id.clone()),
        sha256: Some(offer.artifact.sha256.clone()),
        path: format!("runtime/{}", offer.tool_id),
        active,
    }
}

pub(super) async fn cleanup_remaining(
    prepared: &[(&str, Option<Result<PreparedToolComponent, AppCommandError>>)],
) {
    for (_, result) in prepared {
        if let Some(Ok(component)) = result {
            super::bootstrap_component::cleanup_prepared_component(component).await;
        }
    }
}

pub(super) async fn mark_component_failed(
    data_dir: &Path,
    state: &mut BootstrapState,
    manifest: &InventoryManifest,
    tool_id: &str,
    error: &AppCommandError,
) -> Result<(), AppCommandError> {
    state.set_phase(if manifest.entries.is_empty() {
        InitPhase::Blocked
    } else {
        InitPhase::Degraded
    });
    if let Some(checkpoint) = state
        .components
        .iter_mut()
        .find(|item| item.component_id == tool_id)
    {
        checkpoint.last_error = Some(error.message.clone());
        checkpoint.phase = state.phase;
    }
    write_state(data_dir, state).await
}
