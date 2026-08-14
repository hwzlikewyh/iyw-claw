use std::path::Path;

use sea_orm::DatabaseConnection;

use super::manifest::{
    lock_pending_activations, read_manifest, read_pending_activations, upsert_entry,
    write_manifest, write_pending_activations, InventoryEntry, PendingActivation,
};
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

pub(super) async fn consume_pending_activations(
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<(), AppCommandError> {
    let _guard = lock_pending_activations().await;
    let pending = read_pending_activations(data_dir).await?;
    if pending.is_empty() {
        return Ok(());
    }
    let mut remaining = Vec::new();
    let mut changed = false;
    for item in pending {
        match activate_pending_component(conn, data_dir, &item).await {
            Ok(()) => {
                changed = true;
                tracing::info!(
                    component_id = %item.component_id,
                    version = %item.version,
                    "[agent-version-center] pending activation consumed"
                );
            }
            Err(error) => {
                tracing::warn!(
                    component_id = %item.component_id,
                    version = %item.version,
                    error = %error,
                    "[agent-version-center] pending activation kept for next startup"
                );
                remaining.push(item);
            }
        }
    }
    if changed {
        write_pending_activations(data_dir, &remaining).await?;
    }
    Ok(())
}

pub(crate) async fn consume_pending_agent_activation(
    conn: &DatabaseConnection,
    data_dir: &Path,
    agent_type: AgentType,
) -> Result<bool, AppCommandError> {
    let _guard = lock_pending_activations().await;
    let component_id = serialize_agent_type(agent_type)?;
    let mut pending = read_pending_activations(data_dir).await?;
    let Some(index) = pending
        .iter()
        .position(|item| item.component_kind == "agent" && item.component_id == component_id)
    else {
        return Ok(false);
    };
    let item = pending[index].clone();
    activate_pending_agent(conn, data_dir, agent_type, &item).await?;
    pending.remove(index);
    if let Err(error) = write_pending_activations(data_dir, &pending).await {
        tracing::warn!(
            agent_type = ?agent_type,
            version = %item.version,
            error = %error,
            "[agent-version-center] Agent activated; pending cleanup will retry"
        );
    }
    tracing::info!(
        agent_type = ?agent_type,
        version = %item.version,
        "[agent-version-center] pending Agent activation consumed before launch"
    );
    Ok(true)
}

async fn activate_pending_component(
    conn: &DatabaseConnection,
    data_dir: &Path,
    pending: &PendingActivation,
) -> Result<(), AppCommandError> {
    let policy = pending.policy.as_deref().unwrap_or("recommended");
    let revision = pending.revision.unwrap_or(0);
    match pending.component_kind.as_str() {
        "runtime_tool" => activate_pending_tool(conn, data_dir, pending, policy, revision).await,
        "agent" => {
            let agent_type: AgentType =
                serde_json::from_str(&pending.component_id).map_err(|error| {
                    AppCommandError::configuration_invalid(format!(
                        "Pending agent activation has invalid agent type: {error}"
                    ))
                })?;
            activate_pending_agent(conn, data_dir, agent_type, pending).await
        }
        kind => Err(AppCommandError::invalid_input(format!(
            "Unsupported pending activation component kind: {kind}"
        ))),
    }
}

async fn activate_pending_tool(
    conn: &DatabaseConnection,
    data_dir: &Path,
    pending: &PendingActivation,
    policy: &str,
    revision: u64,
) -> Result<(), AppCommandError> {
    if !super::super::capability::known_tool(&pending.component_id) {
        return Err(AppCommandError::invalid_input(format!(
            "Unknown managed tool in pending activation: {}",
            pending.component_id
        )));
    }
    super::runtime::write_current_pointer(data_dir, &pending.component_id, &pending.version)
        .await?;
    super::super::inventory::activate_tool(
        conn,
        &pending.component_id,
        &pending.version,
        policy,
        revision,
    )
    .await
    .map_err(pending_inventory_error)?;
    mark_manifest_active(data_dir, pending, "runtime").await
}

async fn activate_pending_agent(
    conn: &DatabaseConnection,
    data_dir: &Path,
    agent_type: AgentType,
    pending: &PendingActivation,
) -> Result<(), AppCommandError> {
    validate_local_agent_runtime(conn, agent_type, &pending.version).await?;
    super::super::authorize_agent_version_launch(conn, agent_type, &pending.version).await?;
    super::super::inventory::activate_agent(
        conn,
        agent_type,
        &pending.version,
        pending.policy.as_deref().unwrap_or("manual"),
        pending.revision.unwrap_or(0),
    )
    .await
    .map_err(pending_inventory_error)?;
    if let Err(error) = mark_manifest_active(data_dir, pending, "agents").await {
        tracing::warn!(
            agent_type = ?agent_type,
            version = %pending.version,
            error = %error,
            "[agent-version-center] Agent activated but manifest refresh failed"
        );
    }
    Ok(())
}

pub(crate) async fn validate_local_agent_runtime(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
) -> Result<(), AppCommandError> {
    let installation = super::super::inventory::list_agent_installations(conn, agent_type)
        .await
        .map_err(pending_inventory_error)?
        .into_iter()
        .find(|item| {
            item.version == version
                && item.verified
                && item.platform == crate::acp::registry::current_platform()
        })
        .ok_or_else(|| AppCommandError::invalid_input("Pending Agent version is not ready"))?;
    let paths = crate::acp::agent_storage::AgentStoragePaths::active()
        .ok_or_else(|| AppCommandError::configuration_invalid("Agent storage is unavailable"))?;
    if pending_runtime_ready(&paths, agent_type, version)? {
        return Ok(());
    }
    Err(AppCommandError::invalid_input(format!(
        "Pending Agent runtime is missing for installation {}",
        installation.id
    )))
}

fn pending_runtime_ready(
    paths: &crate::acp::agent_storage::AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
) -> Result<bool, AppCommandError> {
    use crate::acp::registry::AgentDistribution;

    match crate::acp::registry::get_agent_meta(agent_type).distribution {
        AgentDistribution::Binary { cmd, .. } => {
            crate::acp::binary_cache::find_cached_binary_for_agent(paths, agent_type, version, cmd)
                .map(|value| value.is_some())
                .map_err(pending_inventory_error)
        }
        AgentDistribution::Npx { cmd, .. } => Ok(
            crate::acp::npm_runtime::resolve_private_npm_command(paths, agent_type, version, cmd)
                .is_some(),
        ),
        AgentDistribution::Uvx { .. } => Ok(
            crate::acp::binary_cache::is_uvx_agent_version_prepared(paths, agent_type, version)
                && crate::acp::binary_cache::find_cached_uv_tool(paths, "uvx").is_some(),
        ),
    }
}

async fn mark_manifest_active(
    data_dir: &Path,
    pending: &PendingActivation,
    directory: &str,
) -> Result<(), AppCommandError> {
    let mut manifest = read_manifest(data_dir).await?;
    for item in &mut manifest.entries {
        if item.component_id == pending.component_id {
            item.active = false;
        }
    }
    let existing = manifest
        .entries
        .iter_mut()
        .find(|item| item.component_id == pending.component_id && item.version == pending.version);
    if let Some(item) = existing {
        item.active = true;
        item.path = format!("{directory}/{}", pending.component_id);
    } else {
        upsert_entry(&mut manifest, pending_manifest_entry(pending, directory));
    }
    write_manifest(data_dir, &manifest).await
}

fn pending_manifest_entry(pending: &PendingActivation, directory: &str) -> InventoryEntry {
    InventoryEntry {
        component_id: pending.component_id.clone(),
        component_kind: pending.component_kind.clone(),
        version: pending.version.clone(),
        origin: "managed".to_string(),
        artifact_id: None,
        sha256: None,
        path: format!("{directory}/{}", pending.component_id),
        active: true,
    }
}

fn serialize_agent_type(agent_type: AgentType) -> Result<String, AppCommandError> {
    serde_json::to_string(&agent_type).map_err(|error| {
        AppCommandError::configuration_invalid("Agent type cannot be serialized")
            .with_detail(error.to_string())
    })
}

fn pending_inventory_error(error: crate::acp::error::AcpError) -> AppCommandError {
    AppCommandError::task_execution_failed(error.to_string())
}
