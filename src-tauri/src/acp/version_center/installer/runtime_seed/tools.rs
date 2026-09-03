use std::path::Path;

use sea_orm::DatabaseConnection;

use super::{log_component_error, version_at_least, RuntimeSeedImport, SEED_ARTIFACT_PREFIX};
use crate::acp::version_center::capability;
use crate::acp::version_center::inventory::{self, ORIGIN_BUNDLED};
use crate::acp::version_center::types::{ToolArtifact, ToolOffer};
use crate::app_error::AppCommandError;

use super::super::bootstrap_commit::commit_prepared_component;
use super::super::bootstrap_component::{cleanup_prepared_component, PreparedToolComponent};
use super::super::manifest::{read_manifest, OwnershipMarker};
use super::super::runtime::{active_tool_is_healthy, runtime_dir, staging_dir};
use super::super::runtime_seed_files::stage_component;
use super::super::runtime_seed_manifest::{RuntimeSeedComponent, RuntimeSeedManifest};

pub(super) async fn import(
    request: &RuntimeSeedImport<'_>,
    seed_root: &Path,
    seed: &RuntimeSeedManifest,
) -> Vec<String> {
    let mut manifest = match read_manifest(request.data_dir).await {
        Ok(value) => value,
        Err(error) => {
            log_component_error("tool-inventory", "read_manifest", &error);
            return vec![format!(
                "tool-inventory/read_manifest: {}",
                super::error_summary(&error)
            )];
        }
    };
    let mut failures = Vec::new();
    for tool_id in ["node", "git", "uv"] {
        let Some(component) = seed.component(tool_id) else {
            continue;
        };
        if valid_tool_at_least(request.conn, request.data_dir, tool_id, &component.version).await {
            tracing::info!(tool_id, seed_version = %component.version, "[runtime-seed] keeping active tool");
            continue;
        }
        let prepared = match prepare(request.data_dir, seed_root, component).await {
            Ok(value) => value,
            Err(error) => {
                log_component_error(tool_id, "stage", &error);
                failures.push(format!("{tool_id}/stage: {}", super::error_summary(&error)));
                continue;
            }
        };
        let result = commit_prepared_component(
            request.conn,
            request.data_dir,
            &mut manifest,
            &prepared,
            false,
            request.task_id,
            request.emitter,
        )
        .await;
        cleanup_prepared_component(&prepared).await;
        match result {
            Ok(outcome) => tracing::info!(
                tool_id,
                version = %outcome.version,
                "[runtime-seed] tool imported and activated"
            ),
            Err(error) => {
                log_component_error(tool_id, "activate", &error);
                failures.push(format!(
                    "{tool_id}/activate: {}",
                    super::error_summary(&error)
                ));
            }
        }
    }
    failures
}

async fn prepare(
    data_dir: &Path,
    seed_root: &Path,
    component: &RuntimeSeedComponent,
) -> Result<PreparedToolComponent, AppCommandError> {
    let stage = staging_dir(data_dir, &component.id)?;
    let payload = stage.join("payload");
    let copy_seed_root = seed_root.to_path_buf();
    let copy_component = component.clone();
    let copy_payload = payload.clone();
    let result = tokio::task::spawn_blocking(move || {
        stage_component(&copy_seed_root, &copy_component, &copy_payload)
    })
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?;
    if let Err(error) = result {
        let _ = tokio::fs::remove_dir_all(&stage).await;
        return Err(error);
    }
    Ok(PreparedToolComponent::Fresh {
        offer: offer(component)?,
        marker: marker(component),
        origin: ORIGIN_BUNDLED,
        stage,
        payload,
        final_dir: runtime_dir(data_dir, &component.id, &component.version)?,
    })
}

fn offer(component: &RuntimeSeedComponent) -> Result<ToolOffer, AppCommandError> {
    let size = i64::try_from(component.total_size)
        .map_err(|_| AppCommandError::invalid_input("Runtime seed component is too large"))?;
    let artifact_id = format!("{SEED_ARTIFACT_PREFIX}:{}", component.id);
    Ok(ToolOffer {
        revision: 0,
        tool_id: component.id.clone(),
        version_id: artifact_id.clone(),
        version: component.version.clone(),
        channel: "stable".to_string(),
        security_status: "verified".to_string(),
        selection_reason: "bundled_seed".to_string(),
        effective_update_policy: super::SEED_POLICY.to_string(),
        required: true,
        artifact: ToolArtifact {
            id: artifact_id,
            runtime: capability::RUNTIME.to_string(),
            target: capability::current_target().to_string(),
            arch: capability::current_arch().to_string(),
            package_kind: "directory".to_string(),
            size,
            sha256: component.sha256.clone(),
        },
    })
}

fn marker(component: &RuntimeSeedComponent) -> OwnershipMarker {
    OwnershipMarker {
        schema: 1,
        component_id: component.id.clone(),
        component_kind: "runtime_tool".to_string(),
        version: component.version.clone(),
        artifact_id: Some(format!("{SEED_ARTIFACT_PREFIX}:{}", component.id)),
        sha256: Some(component.sha256.clone()),
        target: capability::current_target().to_string(),
        arch: capability::current_arch().to_string(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        origin: ORIGIN_BUNDLED.to_string(),
    }
}

async fn valid_tool_at_least(
    conn: &DatabaseConnection,
    data_dir: &Path,
    tool_id: &str,
    seed: &str,
) -> bool {
    let Ok(settings) = inventory::list_tool_settings(conn).await else {
        return false;
    };
    let active = settings
        .iter()
        .find(|item| item.tool_id == tool_id)
        .and_then(|item| item.active_version.as_deref());
    let Some(active) = active.filter(|version| version_at_least(Some(version), seed)) else {
        return false;
    };
    active_tool_is_healthy(data_dir, tool_id, active).await
}
