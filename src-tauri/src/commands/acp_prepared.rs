use crate::acp::error::AcpError;
use crate::acp::manager::ConnectionManager;
use crate::acp::prepared_session::{PrepareSessionRequest, PreparedSessionHandle};
use crate::db::AppDatabase;
use crate::web::event_bridge::EventEmitter;

pub(crate) async fn prepare_core(
    manager: &ConnectionManager,
    db: &AppDatabase,
    input: (PrepareSessionRequest, String, EventEmitter),
) -> Result<Option<PreparedSessionHandle>, AcpError> {
    let (mut request, owner, emitter) = input;
    if let Some(id) = request.conversation_id {
        let row = crate::db::service::conversation_service::get_by_id(&db.conn, id)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        if row.agent_type != request.agent_type {
            return Err(AcpError::protocol(
                "Conversation preparation target changed",
            ));
        }
        let folder = crate::db::service::folder_service::get_folder_by_id(&db.conn, row.folder_id)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?
            .ok_or_else(|| AcpError::protocol("Conversation workspace no longer exists"))?;
        request.session_id = Some(
            row.external_id
                .ok_or_else(|| AcpError::protocol("Conversation has no recoverable session"))?,
        );
        request.working_dir = Some(folder.path);
    }
    manager.prepare_session(request, (owner, emitter)).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn acp_prepare_session(
    request: PrepareSessionRequest,
    manager: tauri::State<'_, ConnectionManager>,
    db: tauri::State<'_, AppDatabase>,
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Option<PreparedSessionHandle>, AcpError> {
    prepare_core(
        &manager,
        &db,
        (request, window.label().into(), EventEmitter::Tauri(app)),
    )
    .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn acp_cancel_prepared_session(
    preparation_id: String,
    manager: tauri::State<'_, ConnectionManager>,
    window: tauri::WebviewWindow,
) -> Result<(), AcpError> {
    manager
        .cancel_preparation(&preparation_id, window.label())
        .await;
    Ok(())
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn acp_reserve_prepared_workspace(
    preparation_id: String,
    manager: tauri::State<'_, ConnectionManager>,
    window: tauri::WebviewWindow,
) -> Result<Option<String>, AcpError> {
    Ok(manager
        .reserve_prepared_workspace(&preparation_id, window.label())
        .await)
}
