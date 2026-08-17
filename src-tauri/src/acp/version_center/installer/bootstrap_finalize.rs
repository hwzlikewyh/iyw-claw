//! Ordered state and inventory commit for concurrently prepared components.

use std::path::Path;

use sea_orm::DatabaseConnection;

use super::bootstrap_commit::{cleanup_remaining, commit_prepared_component};
use super::bootstrap_component::{
    cleanup_prepared_component, ComponentOutcome, PreparedToolComponent,
};
use super::component::{update_checkpoint, update_checkpoint_deferred};
use super::manifest::InventoryManifest;
use super::state::{write_state, BootstrapState};
use crate::app_error::AppCommandError;
use crate::web::event_bridge::EventEmitter;

pub(super) async fn commit_prepared_components(
    conn: &DatabaseConnection,
    data_dir: &Path,
    manifest: &mut InventoryManifest,
    state: &mut BootstrapState,
    prepared: &mut [(&str, Option<Result<PreparedToolComponent, AppCommandError>>)],
    defer_while_active: bool,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<Vec<String>, (String, AppCommandError)> {
    let mut deferred = Vec::new();
    for index in 0..prepared.len() {
        let (tool_id, result) = take_prepared(prepared, index);
        let component = match result {
            Ok(component) => component,
            Err(error) => return fail(prepared, &tool_id, error).await,
        };
        let outcome = commit_prepared_component(
            conn,
            data_dir,
            manifest,
            &component,
            defer_while_active,
            task_id,
            emitter,
        )
        .await;
        cleanup_prepared_component(&component).await;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => return fail(prepared, &tool_id, error).await,
        };
        checkpoint(state, &tool_id, &mut deferred, outcome);
        if let Err(error) = write_state(data_dir, state).await {
            cleanup_remaining(prepared).await;
            return Err((tool_id, error));
        }
    }
    Ok(deferred)
}

fn take_prepared(
    prepared: &mut [(&str, Option<Result<PreparedToolComponent, AppCommandError>>)],
    index: usize,
) -> (String, Result<PreparedToolComponent, AppCommandError>) {
    let (tool_id, result) = &mut prepared[index];
    (
        (*tool_id).to_string(),
        result.take().expect("prepared component is present"),
    )
}

async fn fail(
    prepared: &[(&str, Option<Result<PreparedToolComponent, AppCommandError>>)],
    tool_id: &str,
    error: AppCommandError,
) -> Result<Vec<String>, (String, AppCommandError)> {
    cleanup_remaining(prepared).await;
    Err((tool_id.to_string(), error))
}

fn checkpoint(
    state: &mut BootstrapState,
    tool_id: &str,
    deferred: &mut Vec<String>,
    outcome: ComponentOutcome,
) {
    if outcome.deferred {
        deferred.push(tool_id.to_string());
        update_checkpoint_deferred(state, tool_id, outcome.version);
    } else {
        update_checkpoint(state, tool_id, outcome.version);
    }
}
