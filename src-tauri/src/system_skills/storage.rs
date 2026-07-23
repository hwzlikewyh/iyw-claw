use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

use super::git::CheckoutInfo;

const AUTO_UPDATE_KEY: &str = "system_skills.auto_update.v1";
const CURRENT_VERSION_KEY: &str = "system_skills.current_version.v1";
const CURRENT_COMMIT_KEY: &str = "system_skills.current_commit.v1";
const PREVIOUS_VERSION_KEY: &str = "system_skills.previous_version.v1";

pub struct StoredState {
    pub auto_update: bool,
    pub current_version: Option<String>,
    pub previous_version: Option<String>,
}

pub async fn load(conn: &DatabaseConnection) -> Result<StoredState, AppCommandError> {
    let auto_update = read(conn, AUTO_UPDATE_KEY)
        .await?
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(true);
    Ok(StoredState {
        auto_update,
        current_version: read(conn, CURRENT_VERSION_KEY).await?,
        previous_version: read(conn, PREVIOUS_VERSION_KEY).await?,
    })
}

pub async fn set_auto_update(
    conn: &DatabaseConnection,
    enabled: bool,
) -> Result<(), AppCommandError> {
    write(conn, AUTO_UPDATE_KEY, &enabled.to_string()).await
}

pub async fn previous_version(conn: &DatabaseConnection) -> Result<String, AppCommandError> {
    read(conn, PREVIOUS_VERSION_KEY)
        .await?
        .ok_or_else(|| AppCommandError::not_found("No previous system skill version is available"))
}

pub async fn persist_install(
    conn: &DatabaseConnection,
    version: &str,
    commit: &str,
    previous: Option<&CheckoutInfo>,
) -> Result<(), AppCommandError> {
    if let Some(previous_version) = previous.and_then(|value| value.version.as_deref()) {
        write(conn, PREVIOUS_VERSION_KEY, previous_version).await?;
    }
    write(conn, CURRENT_VERSION_KEY, version).await?;
    write(conn, CURRENT_COMMIT_KEY, commit).await
}

async fn read(conn: &DatabaseConnection, key: &str) -> Result<Option<String>, AppCommandError> {
    app_metadata_service::get_value(conn, key)
        .await
        .map_err(AppCommandError::from)
}

async fn write(conn: &DatabaseConnection, key: &str, value: &str) -> Result<(), AppCommandError> {
    app_metadata_service::upsert_value(conn, key, value)
        .await
        .map_err(AppCommandError::from)
}
