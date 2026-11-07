mod apply;
mod build;
mod fingerprint;
mod list;
pub(super) mod local;
mod prepare;
mod skills;
mod types;
pub use apply::apply;
pub use list::list;
pub use prepare::inspect;
pub use types::{ApplyInput, InspectInput, ListInput, VersionList, VersionReview};

use crate::app_error::AppCommandError;
use crate::db::service::plugin_installation_service::{self, PluginInstallationRecord};

pub(super) async fn current(
    conn: &sea_orm::DatabaseConnection,
    id: &str,
) -> Result<PluginInstallationRecord, AppCommandError> {
    crate::commands::mcp::require_private_agent_storage_for_write()?;
    crate::commands::mcp_cli_config::account_scope(conn).await?;
    let id = super::types::parse_id(id)?;
    plugin_installation_service::find_by_market_skill_id(conn, id)
        .await
        .map_err(AppCommandError::db)?
        .ok_or_else(|| AppCommandError::not_found("插件已卸载，无法切换本地版本"))
}

pub(super) fn invalid(message: &str) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_versions_list(
    input: ListInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<VersionList, AppCommandError> {
    list(&db.conn, input).await
}
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_version_inspect(
    input: InspectInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<VersionReview, AppCommandError> {
    inspect(&db.conn, input).await
}
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_version_apply(
    input: ApplyInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
    app: tauri::AppHandle,
) -> Result<VersionReview, AppCommandError> {
    apply(
        &db.conn,
        input,
        crate::web::event_bridge::EventEmitter::Tauri(app),
    )
    .await
}
