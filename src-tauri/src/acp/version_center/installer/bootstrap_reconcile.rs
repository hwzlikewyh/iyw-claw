//! Reconciles persisted bootstrap state with the active runtime on disk.

use std::path::Path;

use super::bootstrap_component::healthy_active_version;
use super::component::{empty_checkpoint, update_checkpoint};
use super::manifest::{active_versions, InventoryManifest};
use super::state::{write_state, BootstrapState, InitPhase};
use crate::app_error::AppCommandError;

const TOOL_COMPONENTS: [&str; 3] = ["node", "git", "uv"];

pub(super) async fn reconcile_active_components(
    data_dir: &Path,
    state: &mut BootstrapState,
    manifest: &InventoryManifest,
) -> Result<bool, AppCommandError> {
    let active = active_versions(manifest);
    let mut changed = false;
    for tool_id in TOOL_COMPONENTS {
        let healthy = healthy_active_version(data_dir, tool_id, &active).await;
        match healthy {
            Some(version) if checkpoint_needs_update(state, tool_id, &version) => {
                update_checkpoint(state, tool_id, version);
                changed = true;
            }
            None if checkpoint_needs_clear(state, tool_id) => {
                let mut checkpoint = empty_checkpoint(tool_id);
                checkpoint.version = active.get(tool_id).cloned().unwrap_or_default();
                state.upsert_component(checkpoint);
                changed = true;
            }
            _ => {}
        }
    }
    let ready = components_all_ready(state);
    if ready && state.phase != InitPhase::Ready {
        state.set_phase(InitPhase::Ready);
        changed = true;
    }
    if changed {
        write_state(data_dir, state).await?;
    }
    Ok(ready)
}

fn components_all_ready(state: &BootstrapState) -> bool {
    TOOL_COMPONENTS.iter().all(|tool_id| {
        state
            .component(tool_id)
            .is_some_and(|checkpoint| checkpoint.installed && checkpoint.active)
    })
}

fn checkpoint_needs_update(state: &BootstrapState, tool_id: &str, version: &str) -> bool {
    !state.component(tool_id).is_some_and(|checkpoint| {
        checkpoint.version == version
            && checkpoint.installed
            && checkpoint.active
            && checkpoint.phase == InitPhase::Ready
            && checkpoint.last_error.is_none()
    })
}

fn checkpoint_needs_clear(state: &BootstrapState, tool_id: &str) -> bool {
    state
        .component(tool_id)
        .is_some_and(|checkpoint| checkpoint.installed || checkpoint.active)
}
