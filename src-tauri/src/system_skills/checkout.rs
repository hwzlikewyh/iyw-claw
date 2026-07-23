use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;

use super::git::{self, CheckoutInfo};
use super::manifest;

pub async fn install_validated_tag(
    repo: &Path,
    tag: &str,
    conn: &DatabaseConnection,
    data_dir: &Path,
    previous: Option<&CheckoutInfo>,
) -> Result<(String, Vec<String>), AppCommandError> {
    if previous.is_some() {
        let commit = git::checkout_tag(repo, tag, conn, data_dir).await?;
        let ids = validate_or_restore(repo, tag, previous, conn, data_dir).await?;
        return Ok((commit, ids));
    }
    install_first_checkout(repo, tag, conn, data_dir).await
}

async fn install_first_checkout(
    repo: &Path,
    tag: &str,
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<(String, Vec<String>), AppCommandError> {
    let (staging, commit) = prepare_staging(tag, conn, data_dir).await?;
    let ids = match manifest::validate_checkout(&staging, tag) {
        Ok(ids) => ids,
        Err(error) => {
            remove_entry(&staging)?;
            return Err(error);
        }
    };
    promote_staging(&staging, repo)?;
    Ok((commit, ids))
}

async fn prepare_staging(
    tag: &str,
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<(PathBuf, String), AppCommandError> {
    let staging = super::staging_dir();
    if staging.exists() {
        remove_entry(&staging)?;
    }
    let commit = git::clone_tag(&staging, tag, conn, data_dir).await?;
    Ok((staging, commit))
}

fn promote_staging(staging: &Path, repo: &Path) -> Result<(), AppCommandError> {
    if repo.exists() {
        remove_entry(repo)?;
    }
    std::fs::rename(staging, repo).map_err(AppCommandError::io)
}

async fn validate_or_restore(
    repo: &Path,
    tag: &str,
    previous: Option<&CheckoutInfo>,
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<Vec<String>, AppCommandError> {
    match manifest::validate_checkout(repo, tag) {
        Ok(ids) => Ok(ids),
        Err(error) => {
            restore_previous(repo, previous, conn, data_dir).await;
            Err(error)
        }
    }
}

pub async fn restore_previous(
    repo: &Path,
    previous: Option<&CheckoutInfo>,
    conn: &DatabaseConnection,
    data_dir: &Path,
) {
    let Some(previous) = previous else {
        return;
    };
    if let Err(error) = git::checkout_commit(repo, &previous.commit, conn, data_dir).await {
        tracing::error!(target: "system_skills", "failed to restore previous commit: {error}");
    }
}

fn remove_entry(path: &Path) -> Result<(), AppCommandError> {
    crate::commands::acp::remove_skill_entry(path).map_err(AppCommandError::io)
}
