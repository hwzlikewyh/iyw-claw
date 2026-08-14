#[cfg(feature = "tauri-runtime")]
use std::path::Path;
#[cfg(feature = "tauri-runtime")]
use tauri::Manager;

use crate::app_error::AppCommandError;
use crate::automation::draft::{AutomationDraftSource, DraftRuntime};
#[cfg(feature = "tauri-runtime")]
use crate::db::AppDatabase;
#[cfg(feature = "tauri-runtime")]
use crate::web::event_bridge::EventEmitter;

pub async fn automation_draft_from_conversation_core(
    runtime: DraftRuntime<'_>,
    conversation_id: i32,
) -> Result<AutomationDraftSource, AppCommandError> {
    crate::automation::draft::create_from_conversation(runtime, conversation_id).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn automation_draft_from_conversation(
    app: tauri::AppHandle,
    db: tauri::State<'_, AppDatabase>,
    manager: tauri::State<'_, crate::acp::manager::ConnectionManager>,
    bus: tauri::State<'_, std::sync::Arc<crate::acp::InternalEventBus>>,
    conversation_id: i32,
) -> Result<AutomationDraftSource, AppCommandError> {
    let app_data_dir = app.path().app_data_dir().map_err(|error| {
        AppCommandError::configuration_invalid("Failed to resolve app data directory")
            .with_detail(error.to_string())
    })?;
    let data_dir = crate::paths::resolve_effective_data_dir(Path::new(&app_data_dir));
    automation_draft_from_conversation_core(
        DraftRuntime {
            db: &db,
            manager: &manager,
            bus: bus.as_ref(),
            emitter: EventEmitter::Tauri(app),
            data_dir: &data_dir,
        },
        conversation_id,
    )
    .await
}
