use std::path::Path;
use std::sync::OnceLock;

use chrono::Utc;
use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use crate::app_error::AppCommandError;
use crate::web::event_bridge::EventEmitter;

use super::activation::{self, PendingInstall};
use super::checkout;
use super::git::{self, CheckoutInfo};
use super::manifest;
use super::state::{self, SystemSkillsUpdateLifecycle, SystemSkillsUpdateState};
use super::storage;

fn operation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub async fn snapshot_core(
    conn: &DatabaseConnection,
    data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    hydrate(conn, data_dir, emitter).await?;
    Ok(state::snapshot())
}

pub async fn check_update_core(
    conn: &DatabaseConnection,
    data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    let _guard = operation_lock().lock().await;
    hydrate(conn, data_dir, emitter).await?;
    state::mutate(emitter, |value| {
        value.status = SystemSkillsUpdateLifecycle::Checking;
        value.error = None;
    });
    let latest = match git::latest_stable_tag(conn, data_dir).await {
        Ok(latest) => latest,
        Err(error) => return fail(emitter, error),
    };
    let current = state::snapshot().current_version;
    let repo_managed = super::repository_dir().join(".git").is_dir();
    let available = if repo_managed {
        git::is_newer(current.as_deref(), &latest.version)
    } else {
        latest.version >= manifest::embedded_version()?
    };
    Ok(state::mutate(emitter, |value| {
        value.latest_version = Some(latest.name);
        value.last_checked_at = Some(Utc::now().to_rfc3339());
        value.status = if available {
            SystemSkillsUpdateLifecycle::UpdateAvailable
        } else {
            SystemSkillsUpdateLifecycle::UpToDate
        };
    }))
}

pub async fn apply_update_core(
    conn: &DatabaseConnection,
    data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    let _guard = operation_lock().lock().await;
    hydrate(conn, data_dir, emitter).await?;
    match apply_update_locked(conn, data_dir, emitter).await {
        Ok(result) => Ok(result),
        Err(error) => fail(emitter, error),
    }
}

async fn apply_update_locked(
    conn: &DatabaseConnection,
    data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    let latest = git::latest_stable_tag(conn, data_dir).await?;
    let repo = super::repository_dir();
    let previous = checkout_if_present(&repo, conn, data_dir).await?;
    if previous.is_none() && latest.version < manifest::embedded_version()? {
        return Err(AppCommandError::configuration_invalid(
            "The latest remote system skills are older than the embedded version",
        ));
    }
    if previous.as_ref().is_some_and(|checkout| checkout.dirty) {
        return Ok(mark_dirty(emitter));
    }
    state::mutate(emitter, |value| {
        value.status = SystemSkillsUpdateLifecycle::Downloading;
        value.latest_version = Some(latest.name.clone());
        value.error = None;
    });
    let (commit, ids) =
        checkout::install_validated_tag(&repo, &latest.name, conn, data_dir, previous.as_ref())
            .await?;
    activation::finish(PendingInstall {
        conn,
        data_dir,
        emitter,
        version: &latest.name,
        commit: &commit,
        ids: &ids,
        previous,
    })
    .await
}

pub async fn set_auto_update_core(
    conn: &DatabaseConnection,
    enabled: bool,
    emitter: &EventEmitter,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    storage::set_auto_update(conn, enabled).await?;
    Ok(state::mutate(emitter, |value| value.auto_update = enabled))
}

pub async fn rollback_core(
    conn: &DatabaseConnection,
    data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    let _guard = operation_lock().lock().await;
    hydrate(conn, data_dir, emitter).await?;
    match rollback_locked(conn, data_dir, emitter).await {
        Ok(result) => Ok(result),
        Err(error) => fail(emitter, error),
    }
}

async fn rollback_locked(
    conn: &DatabaseConnection,
    data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    let previous_version = storage::previous_version(conn).await?;
    let repo = super::repository_dir();
    let current = git::inspect_checkout(&repo, conn, data_dir).await?;
    if current.dirty {
        return Ok(mark_dirty(emitter));
    }
    state::mutate(emitter, |value| {
        value.status = SystemSkillsUpdateLifecycle::Applying;
        value.error = None;
    });
    let (commit, ids) =
        checkout::install_validated_tag(&repo, &previous_version, conn, data_dir, Some(&current))
            .await?;
    activation::finish(PendingInstall {
        conn,
        data_dir,
        emitter,
        version: &previous_version,
        commit: &commit,
        ids: &ids,
        previous: Some(current),
    })
    .await
}

pub async fn startup_update_core(
    conn: &DatabaseConnection,
    data_dir: &Path,
    emitter: &EventEmitter,
) {
    let snapshot = match snapshot_core(conn, data_dir, emitter).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::warn!(target: "system_skills", "startup state load failed: {error}");
            return;
        }
    };
    if !snapshot.auto_update {
        tracing::debug!(target: "system_skills", "startup update skipped: disabled");
        return;
    }
    let checked = match check_update_core(conn, data_dir, emitter).await {
        Ok(checked) => checked,
        Err(error) => {
            tracing::warn!(target: "system_skills", "startup update check failed: {error}");
            return;
        }
    };
    if checked.status == SystemSkillsUpdateLifecycle::UpdateAvailable {
        match apply_update_core(conn, data_dir, emitter).await {
            Ok(result) => tracing::info!(
                target: "system_skills",
                version = ?result.current_version,
                "startup update applied"
            ),
            Err(error) => {
                tracing::warn!(target: "system_skills", "startup update failed: {error}")
            }
        }
    } else {
        tracing::debug!(
            target: "system_skills",
            version = ?checked.current_version,
            "system skills are up to date"
        );
    }
}

async fn hydrate(
    conn: &DatabaseConnection,
    data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<(), AppCommandError> {
    let stored = storage::load(conn).await?;
    let checkout = checkout_if_present(&super::repository_dir(), conn, data_dir).await?;
    let embedded_version = format!("v{}", manifest::embedded_version()?);
    state::mutate(emitter, |value| {
        value.auto_update = stored.auto_update;
        if let Some(checkout) = checkout.as_ref() {
            value.current_version = checkout.version.clone().or(stored.current_version);
            value.current_commit = Some(checkout.commit.clone());
            value.previous_version = stored.previous_version;
        } else {
            value.current_version = Some(embedded_version);
            value.current_commit = None;
            value.previous_version = None;
        }
        value.dirty = checkout.is_some_and(|item| item.dirty);
    });
    Ok(())
}

async fn checkout_if_present(
    repo: &Path,
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<Option<CheckoutInfo>, AppCommandError> {
    if !repo.join(".git").is_dir() {
        return Ok(None);
    }
    git::inspect_checkout(repo, conn, data_dir).await.map(Some)
}

fn mark_dirty(emitter: &EventEmitter) -> SystemSkillsUpdateState {
    state::mutate(emitter, |value| {
        value.status = SystemSkillsUpdateLifecycle::BlockedDirty;
        value.dirty = true;
        value.error = Some("Tracked system skill files have local changes".to_string());
    })
}

fn record_error(emitter: &EventEmitter, error: &AppCommandError) -> AppCommandError {
    let message = error
        .detail
        .as_deref()
        .filter(|detail| *detail != error.message)
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or_else(|| error.message.clone());
    state::mutate(emitter, |value| {
        value.status = SystemSkillsUpdateLifecycle::Error;
        value.error = Some(message);
    });
    error.clone()
}

fn fail(
    emitter: &EventEmitter,
    error: AppCommandError,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    Err(record_error(emitter, &error))
}
