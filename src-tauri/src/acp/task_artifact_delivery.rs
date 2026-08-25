use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use sea_orm::DatabaseConnection;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::acp::session_state::SessionState;
use crate::acp::types::AcpEvent;
use crate::db::service::task_artifact_service;
use crate::web::event_bridge::{emit_with_state, EventEmitter};

mod path;
use path::{create_managed_directory, turn_directory, verify_managed_directory, TurnIdentity};

const MAX_DIRECT_CHILDREN: usize = 100;
const UNAVAILABLE_CONTEXT_BODY: &str = "## Current-turn final artifact delivery\n\nThe managed artifact directory is unavailable for this turn. If the task produces a final user-facing file, directory, or HTTP/HTTPS URL, use `present_task_files` before the final response to add it to the current conversation Artifacts. Do not register source, config, tests, build output, caches, logs, temporary files, or internal work unless the user explicitly requested that item as the final deliverable.";

pub(crate) struct CompletedTurnDelivery<'a> {
    pub db: &'a DatabaseConnection,
    pub state: &'a Arc<RwLock<SessionState>>,
    pub emitter: &'a EventEmitter,
    pub connection_id: &'a str,
    pub conversation_id: i32,
    pub turn_generation: i64,
}

pub(crate) async fn prepare_turn_context(
    state: &Arc<RwLock<SessionState>>,
    connection_id: &str,
) -> Option<Arc<str>> {
    let (conversation_id, turn_generation) = {
        let snapshot = state.read().await;
        (
            snapshot.conversation_id?,
            snapshot.turn_generation.saturating_add(1),
        )
    };
    let identity = TurnIdentity::new(connection_id, conversation_id, turn_generation);
    let directory = match turn_directory(&identity) {
        Ok(directory) => directory,
        Err(error) => {
            log_failure(("prepare_path", &identity, None), &error);
            return Some(private_context(UNAVAILABLE_CONTEXT_BODY));
        }
    };
    match create_managed_directory(&identity).await {
        Ok(()) => Some(managed_context(&directory)),
        Err(error) => {
            log_failure(("create_directory", &identity, Some(&directory)), &error);
            Some(private_context(UNAVAILABLE_CONTEXT_BODY))
        }
    }
}

pub(crate) async fn deliver_completed_turn(context: CompletedTurnDelivery<'_>) {
    let started = Instant::now();
    let generation_advanced =
        match crate::db::service::conversation_service::mark_completed_turn_generation(
            context.db,
            context.conversation_id,
            context.turn_generation,
        )
        .await
        {
            Ok(advanced) => advanced,
            Err(error) => {
                tracing::error!(
                    connection_id = context.connection_id,
                    conversation_id = context.conversation_id,
                    turn_generation = context.turn_generation,
                    error = %error,
                    "[task-artifacts] completed turn generation update failed"
                );
                false
            }
        };
    if generation_advanced {
        crate::commands::task_artifacts::emit_task_artifacts_changed(
            context.emitter,
            context.conversation_id,
        );
    }
    let identity = TurnIdentity::new(
        context.connection_id,
        context.conversation_id,
        context.turn_generation,
    );
    let directory = match turn_directory(&identity) {
        Ok(directory) => directory,
        Err(error) => {
            log_failure(("resolve_delivery_path", &identity, None), &error);
            emit_delivery_error(context.state, context.emitter, false).await;
            return;
        }
    };
    let selection = match scan_direct_children(&identity, &directory).await {
        Ok(selection) => selection,
        Err(error) => {
            log_failure(("scan_directory", &identity, Some(&directory)), &error);
            emit_delivery_error(context.state, context.emitter, false).await;
            return;
        }
    };
    if selection.paths.is_empty() {
        tracing::debug!(
            connection_id = context.connection_id,
            conversation_id = context.conversation_id,
            turn_generation = context.turn_generation,
            elapsed_ms = started.elapsed().as_millis(),
            "[task-artifacts] managed turn directory contained no deliverables"
        );
        return;
    }
    register_selection(&context, &directory, selection).await;
}

struct ScanSelection {
    paths: Vec<String>,
    whole_directory: bool,
}

type RegistrationOutcome = (Value, bool, Instant);

async fn scan_direct_children(
    identity: &TurnIdentity<'_>,
    directory: &Path,
) -> io::Result<ScanSelection> {
    verify_managed_directory(identity, directory).await?;
    let mut entries = tokio::fs::read_dir(directory).await?;
    let mut paths = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Ok(whole_directory_selection());
        };
        paths.push(name.to_string());
        if paths.len() > MAX_DIRECT_CHILDREN {
            return Ok(whole_directory_selection());
        }
    }
    paths.sort();
    Ok(ScanSelection {
        paths,
        whole_directory: false,
    })
}

fn whole_directory_selection() -> ScanSelection {
    ScanSelection {
        paths: vec![".".to_string()],
        whole_directory: true,
    }
}

async fn register_selection(
    context: &CompletedTurnDelivery<'_>,
    directory: &Path,
    selection: ScanSelection,
) {
    let started = Instant::now();
    let whole_directory = selection.whole_directory;
    let result = task_artifact_service::register_artifacts(
        context.db,
        context.conversation_id,
        Some(context.turn_generation),
        directory,
        selection.paths,
    )
    .await;
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            let identity = TurnIdentity::new(
                context.connection_id,
                context.conversation_id,
                context.turn_generation,
            );
            log_failure(("persist", &identity, Some(directory)), &error);
            emit_delivery_error(context.state, context.emitter, false).await;
            return;
        }
    };
    process_registration(context, (result, whole_directory, started)).await;
}

async fn process_registration(context: &CompletedTurnDelivery<'_>, outcome: RegistrationOutcome) {
    let (result, whole_directory, started) = outcome;
    let result_count = |key| {
        result
            .get(key)
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    let accepted = result_count("accepted");
    let rejected = result_count("rejected");
    if accepted > 0 {
        crate::commands::task_artifacts::emit_task_artifacts_changed(
            context.emitter,
            context.conversation_id,
        );
    }
    tracing::info!(
        connection_id = context.connection_id,
        conversation_id = context.conversation_id,
        turn_generation = context.turn_generation,
        accepted,
        rejected,
        whole_directory,
        elapsed_ms = started.elapsed().as_millis(),
        "[task-artifacts] managed turn delivery processed"
    );
    if rejected > 0 {
        tracing::warn!(
            connection_id = context.connection_id,
            conversation_id = context.conversation_id,
            turn_generation = context.turn_generation,
            rejected,
            reasons = %rejection_reasons(&result),
            "[task-artifacts] managed deliverables were rejected"
        );
        emit_delivery_error(context.state, context.emitter, true).await;
    }
}

fn rejection_reasons(result: &Value) -> String {
    result
        .get("rejected")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("reason").and_then(Value::as_str))
        .take(5)
        .collect::<Vec<_>>()
        .join(",")
}

async fn emit_delivery_error(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    partial: bool,
) {
    let agent_type = state.read().await.agent_type.to_string();
    let message = if partial {
        "Some final task deliverables could not be added to the current conversation Artifacts."
    } else {
        "Automatic delivery of final task artifacts to the current conversation failed."
    };
    emit_with_state(
        state,
        emitter,
        AcpEvent::Error {
            message: message.to_string(),
            agent_type,
            code: None,
            details: None,
            terminal: false,
        },
    )
    .await;
}

fn managed_context(directory: &Path) -> Arc<str> {
    private_context(&format!(
        "## Current-turn final artifact delivery\n\nManaged directory for final user-facing file or directory deliverables in this turn:\n{}\n\nIf the task produces final deliverables and the user did not choose another output location, write them under this directory before the final response. Put only final deliverables there as direct children; a deliverable directory may contain its own files. Do not place source, config, tests, build output, caches, logs, temporary files, or internal work there unless the user explicitly requested that item as the final deliverable. Leave the directory empty when the task has no file or directory deliverable. The host registers its direct children only after a successful turn.",
        directory.display()
    ))
}

fn private_context(body: &str) -> Arc<str> {
    Arc::from(format!(
        "{}\n{}\n{}",
        crate::user_memory::USER_CONTEXT_START,
        body,
        crate::user_memory::USER_CONTEXT_END,
    ))
}

fn log_failure(scope: (&str, &TurnIdentity<'_>, Option<&Path>), error: &dyn std::fmt::Display) {
    let (stage, identity, directory) = scope;
    tracing::error!(
        stage,
        connection_id = identity.connection_id,
        conversation_id = identity.conversation_id,
        turn_generation = identity.turn_generation,
        directory = directory.map(|path| path.display().to_string()),
        error = %error,
        "[task-artifacts] managed turn artifact operation failed"
    );
}
