use std::sync::LazyLock;

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

use super::InstallTask;

const JOURNAL_KEY: &str = "mcp.install.journal.v1";
const JOURNAL_LIMIT: usize = 32;
static WRITE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(super) async fn read(conn: &DatabaseConnection) -> Result<Vec<InstallTask>, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, JOURNAL_KEY)
        .await
        .map_err(AppCommandError::db)?;
    raw.map(|value| {
        serde_json::from_str(&value)
            .map_err(|_| AppCommandError::configuration_invalid("安装任务记录无效"))
    })
    .unwrap_or_else(|| Ok(Vec::new()))
}

pub(super) async fn save(
    conn: &DatabaseConnection,
    view: InstallTask,
) -> Result<(), AppCommandError> {
    let _guard = WRITE_LOCK.lock().await;
    let mut entries = read(conn).await?;
    super::project_entries(&mut entries)?;
    entries.retain(|entry| entry.id != view.id);
    entries.insert(0, view);
    entries.sort_by(|left, right| {
        super::active(&right.phase)
            .cmp(&super::active(&left.phase))
            .then_with(|| right.created_at.cmp(&left.created_at))
    });
    entries.truncate(JOURNAL_LIMIT);
    let raw = serde_json::to_string(&entries)
        .map_err(|_| AppCommandError::configuration_invalid("无法保存安装任务记录"))?;
    app_metadata_service::upsert_value(conn, JOURNAL_KEY, &raw)
        .await
        .map_err(AppCommandError::db)
}
