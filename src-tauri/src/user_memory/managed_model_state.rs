use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use super::managed_model_archive::{validate_directory, validate_layout};
use super::managed_model_types::{model_error, MODEL_ID};
use crate::app_error::{AppCommandError, AppErrorCode};
use crate::db::service::app_metadata_service;

const CURRENT_FILE: &str = "current.json";
const RETRY_KEY: &str = "user_memory.managed_model_retry_v1";
const RETRY_DELAYS_SECONDS: [i64; 5] = [60, 300, 1_800, 7_200, 21_600];

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CurrentModelPointer {
    schema_version: u32,
    model_id: String,
    version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RetryState {
    pub model_id: String,
    pub desired_version: String,
    pub attempt_count: u32,
    pub next_attempt_at: DateTime<Utc>,
    pub last_error_code: String,
    pub updated_at: DateTime<Utc>,
}

pub(super) fn managed_root(data_dir: &Path) -> PathBuf {
    let user_root = crate::paths::iyw_claw_user_dir();
    let root = if user_root.is_absolute() {
        user_root
    } else {
        data_dir.to_path_buf()
    };
    root.join("runtime/models/memory-embedding")
}

pub(super) fn current_directory(data_dir: &Path) -> Result<PathBuf, AppCommandError> {
    let root = managed_root(data_dir);
    let pointer: CurrentModelPointer =
        super::structured_file::read_json_optional(&root, CURRENT_FILE, 2_048)?
            .ok_or_else(|| model_error("Managed model pointer is missing"))?;
    validate_pointer(&pointer)?;
    let directory = root.join(&pointer.model_id).join(&pointer.version);
    validate_directory(&directory)?;
    Ok(directory)
}

pub(super) fn installed_version(data_dir: &Path) -> Option<String> {
    let root = managed_root(data_dir);
    let pointer = super::structured_file::read_json_optional::<CurrentModelPointer>(
        &root,
        CURRENT_FILE,
        2_048,
    )
    .ok()
    .flatten()?;
    validate_pointer(&pointer).ok()?;
    validate_directory(&root.join(&pointer.model_id).join(&pointer.version)).ok()?;
    Some(pointer.version)
}

pub(super) fn installed_quick(data_dir: &Path) -> bool {
    let root = managed_root(data_dir);
    let pointer = super::structured_file::read_json_optional::<CurrentModelPointer>(
        &root,
        CURRENT_FILE,
        2_048,
    )
    .ok()
    .flatten();
    pointer.is_some_and(|pointer| {
        validate_pointer(&pointer).is_ok()
            && validate_layout(&root.join(pointer.model_id).join(pointer.version)).is_ok()
    })
}

pub(super) fn create_stage(data_dir: &Path) -> Result<PathBuf, AppCommandError> {
    let root = managed_root(data_dir);
    ensure_directory(&root)?;
    let staging = root.join(".staging");
    ensure_directory(&staging)?;
    let stage = staging.join(uuid::Uuid::new_v4().simple().to_string());
    std::fs::create_dir(&stage).map_err(AppCommandError::io)?;
    Ok(stage)
}

pub(super) fn commit_stage(
    data_dir: &Path,
    stage: &Path,
    version: &str,
) -> Result<PathBuf, AppCommandError> {
    validate_version(version)?;
    validate_directory(stage)?;
    let root = managed_root(data_dir);
    let model_root = root.join(MODEL_ID);
    ensure_directory(&model_root)?;
    let destination = model_root.join(version);
    if destination.exists() {
        if validate_directory(&destination).is_ok() {
            std::fs::remove_dir_all(stage).map_err(AppCommandError::io)?;
        } else {
            std::fs::remove_dir_all(&destination).map_err(AppCommandError::io)?;
            std::fs::rename(stage, &destination).map_err(AppCommandError::io)?;
        }
    } else {
        std::fs::rename(stage, &destination).map_err(AppCommandError::io)?;
    }
    let pointer = CurrentModelPointer {
        schema_version: 1,
        model_id: MODEL_ID.to_string(),
        version: version.to_string(),
    };
    super::structured_file::write_json_atomic(&root, CURRENT_FILE, &pointer)?;
    Ok(destination)
}

pub(super) async fn load_retry(
    conn: &DatabaseConnection,
) -> Result<Option<RetryState>, AppCommandError> {
    let Some(raw) = app_metadata_service::get_value(conn, RETRY_KEY)
        .await
        .map_err(AppCommandError::from)?
    else {
        return Ok(None);
    };
    serde_json::from_str::<Option<RetryState>>(&raw).map_err(|error| {
        AppCommandError::configuration_invalid("Managed model retry state is invalid")
            .with_detail(error.to_string())
    })
}

pub(super) async fn record_failure(
    conn: &DatabaseConnection,
    desired_version: &str,
    error: &AppCommandError,
) -> Result<RetryState, AppCommandError> {
    let previous = load_retry(conn)
        .await?
        .filter(|state| state.model_id == MODEL_ID);
    let attempt_count = previous.map_or(1, |state| state.attempt_count.saturating_add(1));
    let now = Utc::now();
    let state = RetryState {
        model_id: MODEL_ID.to_string(),
        desired_version: desired_version.to_string(),
        attempt_count,
        next_attempt_at: now + retry_delay(attempt_count),
        last_error_code: error_code(error.code),
        updated_at: now,
    };
    persist_retry(conn, Some(&state)).await?;
    Ok(state)
}

pub(super) async fn clear_retry(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    persist_retry(conn, None).await
}

async fn persist_retry(
    conn: &DatabaseConnection,
    state: Option<&RetryState>,
) -> Result<(), AppCommandError> {
    let value = serde_json::to_string(&state).map_err(|error| {
        AppCommandError::configuration_invalid("Serialize managed model retry state")
            .with_detail(error.to_string())
    })?;
    app_metadata_service::upsert_value(conn, RETRY_KEY, &value)
        .await
        .map_err(AppCommandError::from)
}

fn retry_delay(attempt_count: u32) -> Duration {
    let index = attempt_count.saturating_sub(1) as usize;
    let seconds = RETRY_DELAYS_SECONDS[index.min(RETRY_DELAYS_SECONDS.len() - 1)];
    let jitter_limit = (seconds / 10).max(1);
    let jitter = rand::thread_rng().gen_range(-jitter_limit..=jitter_limit);
    Duration::seconds(seconds + jitter)
}

fn validate_pointer(pointer: &CurrentModelPointer) -> Result<(), AppCommandError> {
    if pointer.schema_version != 1 || pointer.model_id != MODEL_ID {
        return Err(model_error("Managed model pointer identity is invalid"));
    }
    validate_version(&pointer.version)
}

fn validate_version(version: &str) -> Result<(), AppCommandError> {
    let valid = !version.is_empty()
        && version.len() <= 64
        && version
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'));
    valid
        .then_some(())
        .ok_or_else(|| model_error("Managed model version is invalid"))
}

fn ensure_directory(path: &Path) -> Result<(), AppCommandError> {
    super::helpers::reject_symlink(path)?;
    std::fs::create_dir_all(path).map_err(AppCommandError::io)?;
    super::helpers::reject_symlink(path)
}

fn error_code(code: AppErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}
