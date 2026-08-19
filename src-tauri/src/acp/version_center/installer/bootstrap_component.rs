//! Bootstrap preparation that may run concurrently without mutating inventory.

use sea_orm::DatabaseConnection;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::init::emit_init_event;
use super::manifest::{marker_matches, read_marker, OwnershipMarker};
use super::runtime::{active_tool_is_healthy, runtime_dir};
use crate::acp::version_center::capability;
use crate::acp::version_center::client::AgentPlatformClient;
use crate::acp::version_center::inventory::ORIGIN_MANAGED;
use crate::acp::version_center::types::{ResolveToolRequest, ToolOffer};
use crate::app_error::AppCommandError;
use crate::web::event_bridge::EventEmitter;

pub(super) struct ComponentOutcome {
    pub version: String,
    pub deferred: bool,
}

pub(super) enum PreparedToolComponent {
    Keep {
        version: String,
    },
    Deferred {
        offer: ToolOffer,
    },
    Fresh {
        offer: ToolOffer,
        marker: OwnershipMarker,
        origin: &'static str,
        stage: PathBuf,
        payload: PathBuf,
        final_dir: PathBuf,
    },
}

pub(super) async fn prepare_tool_components(
    conn: &DatabaseConnection,
    data_dir: &Path,
    channel: &str,
    defer_while_active: bool,
    task_id: &str,
    emitter: &EventEmitter,
    active: &BTreeMap<String, String>,
) -> [Result<PreparedToolComponent, AppCommandError>; 3] {
    tokio::join!(
        prepare_tool_component(
            conn,
            data_dir,
            "node",
            channel,
            defer_while_active,
            task_id,
            emitter,
            active
        ),
        prepare_tool_component(
            conn,
            data_dir,
            "git",
            channel,
            defer_while_active,
            task_id,
            emitter,
            active
        ),
        prepare_tool_component(
            conn,
            data_dir,
            "uv",
            channel,
            defer_while_active,
            task_id,
            emitter,
            active
        ),
    )
    .into()
}

#[allow(clippy::too_many_arguments)]
async fn prepare_tool_component(
    conn: &DatabaseConnection,
    data_dir: &Path,
    tool_id: &str,
    channel: &str,
    defer_while_active: bool,
    task_id: &str,
    emitter: &EventEmitter,
    active: &BTreeMap<String, String>,
) -> Result<PreparedToolComponent, AppCommandError> {
    emit_init_event(emitter, task_id, "resolving", Some(tool_id), "");
    let current_version = active.get(tool_id).cloned().unwrap_or_default();
    let healthy_version = healthy_active_version(data_dir, tool_id, active).await;
    let offer = match resolve_offer(conn, tool_id, &current_version, channel).await {
        Ok(offer) => offer,
        Err(error) => {
            let Some(version) = healthy_version else {
                return Err(error);
            };
            tracing::warn!(
                tool_id,
                version,
                error_code = ?error.code,
                "[agent-version-center] resolve unavailable; keeping healthy active tool"
            );
            return Ok(PreparedToolComponent::Keep { version });
        }
    };
    if let Some(version) =
        healthy_version.filter(|version| version_at_least(version, &offer.version))
    {
        tracing::info!(
            tool_id,
            active_version = %version,
            offered_version = %offer.version,
            "[agent-version-center] active tool satisfies offer; skipping update"
        );
        return Ok(PreparedToolComponent::Keep { version });
    }
    let marker = ownership_marker(tool_id, &offer);
    let final_dir = runtime_dir(data_dir, tool_id, &offer.version)?;
    let marker_ok = read_marker(&final_dir)
        .await
        .is_some_and(|existing| marker_matches(&existing, &marker));
    if marker_ok
        && active
            .get(tool_id)
            .is_some_and(|version| version == &offer.version)
    {
        return Ok(PreparedToolComponent::Keep {
            version: offer.version,
        });
    }
    if defer_while_active && marker_ok {
        return Ok(PreparedToolComponent::Deferred { offer });
    }
    super::bootstrap_download::prepare_fresh(
        conn,
        data_dir,
        tool_id,
        channel,
        task_id,
        emitter,
        current_version,
        offer,
        marker,
        final_dir,
    )
    .await
}

pub(super) async fn healthy_active_version(
    data_dir: &Path,
    tool_id: &str,
    active: &BTreeMap<String, String>,
) -> Option<String> {
    let version = active.get(tool_id)?;
    active_tool_is_healthy(data_dir, tool_id, version)
        .await
        .then(|| version.clone())
}

fn version_at_least(active: &str, offered: &str) -> bool {
    let (Ok(active), Ok(offered)) = (
        semver::Version::parse(active),
        semver::Version::parse(offered),
    ) else {
        return false;
    };
    active >= offered
}

async fn resolve_offer(
    conn: &DatabaseConnection,
    tool_id: &str,
    current_version: &str,
    channel: &str,
) -> Result<ToolOffer, AppCommandError> {
    AgentPlatformClient::resolve_tool(
        conn,
        ResolveToolRequest {
            tool_id,
            current_version,
            requested_version: None,
            pinned_version: None,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: capability::RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
            reason: "automatic",
        },
    )
    .await
}

fn ownership_marker(tool_id: &str, offer: &ToolOffer) -> OwnershipMarker {
    OwnershipMarker {
        schema: 1,
        component_id: tool_id.to_string(),
        component_kind: "runtime_tool".to_string(),
        version: offer.version.clone(),
        artifact_id: Some(offer.artifact.id.clone()),
        sha256: Some(offer.artifact.sha256.clone()),
        target: capability::current_target().to_string(),
        arch: capability::current_arch().to_string(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        origin: ORIGIN_MANAGED.to_string(),
    }
}

pub(super) async fn cleanup_prepared_component(component: &PreparedToolComponent) {
    if let PreparedToolComponent::Fresh { stage, .. } = component {
        super::bootstrap_download::remove_stage(stage).await;
    }
}
