use std::path::Path;

use crate::db::error::DbError;
use crate::db::service::folder_service;
use crate::db::AppDatabase;
use crate::models::FolderDetail;

const AUTOMATION_DIR: &str = "automations";

pub async fn ensure_default_folder(
    db: &AppDatabase,
    data_dir: &Path,
    automation_id: i32,
) -> Result<FolderDetail, DbError> {
    let path = data_dir
        .join(AUTOMATION_DIR)
        .join(format!("automation-{automation_id}"));
    tracing::info!(
        automation_id,
        "[automation] ensuring dedicated default folder"
    );

    if let Err(error) = tokio::fs::create_dir_all(&path).await {
        tracing::error!(
            automation_id,
            error = %error,
            "[automation] failed to create default folder"
        );
        return Err(error.into());
    }

    let path_text = path.to_string_lossy().into_owned();
    let entry = folder_service::add_folder(&db.conn, &path_text)
        .await
        .map_err(|error| {
            tracing::error!(
                automation_id,
                error = %error,
                "[automation] failed to register default folder"
            );
            error
        })?;
    let detail = folder_service::get_folder_by_id(&db.conn, entry.id)
        .await?
        .ok_or_else(|| DbError::NotFound("default automation folder".to_string()))?;
    tracing::info!(
        automation_id,
        folder_id = detail.id,
        "[automation] dedicated default folder ready"
    );
    Ok(detail)
}
