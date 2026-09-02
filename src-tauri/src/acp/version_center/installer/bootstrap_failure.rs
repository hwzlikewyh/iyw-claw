use std::path::Path;

use super::component::empty_checkpoint;
use super::manifest::InventoryManifest;
use super::state::{write_state, BootstrapState, InitPhase};
use crate::app_error::AppCommandError;

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
    let mut checkpoint = state
        .component(tool_id)
        .cloned()
        .unwrap_or_else(|| empty_checkpoint(tool_id));
    let had_healthy_active = checkpoint.installed && checkpoint.active;
    checkpoint.last_error = Some(error_summary(error));
    checkpoint.phase = state.phase;
    if !had_healthy_active {
        checkpoint.installed = false;
        checkpoint.active = false;
    }
    state.upsert_component(checkpoint);
    write_state(data_dir, state).await
}

fn error_summary(error: &AppCommandError) -> String {
    let detail = error.detail.as_deref().unwrap_or_default();
    let summary = if detail.trim().is_empty() {
        error.message.clone()
    } else {
        format!("{}: {detail}", error.message)
    };
    crate::acp::stderr_tail::sanitize_diagnostic(&summary)
        .chars()
        .take(512)
        .collect()
}
