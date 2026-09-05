use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use crate::acp::agent_storage::{load_config, save_config, AgentStorageConfig, AgentStorageError};

const APP_DIR_NAME: &str = "app";
const INSTALL_ROOT_METADATA_KEY: &str = "desktop.install_root.v1";
const REBASED_PROFILE_FILES: [&str; 2] = ["config/codex/config.toml", "config/hermes/config.yaml"];

pub async fn reconcile(
    conn: &DatabaseConnection,
    selected_root: &Path,
) -> Result<Option<PathBuf>, AgentStorageError> {
    let Some(mut config) = load_config(conn).await? else {
        return Ok(None);
    };
    let recorded_root =
        crate::db::service::app_metadata_service::get_value(conn, INSTALL_ROOT_METADATA_KEY)
            .await?
            .map(PathBuf::from);
    let Some(source) = config.root.clone().filter(|_| config.initialized) else {
        record_install_root(conn, selected_root).await;
        return Ok(None);
    };
    let stale_profile_root = discover_stale_profile_root(selected_root);
    if same_path(&source, selected_root) {
        let Some(previous_root) = stale_profile_root else {
            record_install_root(conn, selected_root).await;
            return Ok(None);
        };
        commit_storage_rebase(conn, &mut config, selected_root, &previous_root, false).await?;
        record_install_root(conn, selected_root).await;
        return Ok(Some(previous_root));
    }
    let profile_matches_source = stale_profile_root
        .as_ref()
        .is_some_and(|root| same_path(root, &source));
    if !profile_matches_source && !is_previous_install_root(recorded_root.as_deref(), &source) {
        record_install_root(conn, selected_root).await;
        return Ok(None);
    }
    commit_storage_rebase(conn, &mut config, selected_root, &source, true).await?;
    record_install_root(conn, selected_root).await;
    Ok(Some(source))
}

async fn commit_storage_rebase(
    conn: &DatabaseConnection,
    config: &mut AgentStorageConfig,
    selected_root: &Path,
    previous_root: &Path,
    update_root: bool,
) -> Result<(), AgentStorageError> {
    let original = config.clone();
    rebase_profile_overrides(config, previous_root, selected_root);
    let profile_changes = prepare_profile_path_changes(selected_root, previous_root)?;
    if update_root {
        config.root = Some(selected_root.to_path_buf());
    }
    save_config(conn, config).await?;
    if let Err(error) = apply_profile_path_changes(&profile_changes) {
        let _ = save_config(conn, &original).await;
        rollback_profile_path_changes(&profile_changes);
        return Err(error);
    }
    Ok(())
}

fn is_previous_install_root(recorded: Option<&Path>, source: &Path) -> bool {
    recorded.is_some_and(|root| same_path(root, source))
        || (source.join(APP_DIR_NAME).is_dir()
            && source.join("data").is_dir()
            && source.join("logs").is_dir())
}

fn rebase_profile_overrides(config: &mut AgentStorageConfig, source: &Path, selected: &Path) {
    for path in config.profile_overrides.values_mut() {
        if let Ok(relative) = path.strip_prefix(source) {
            *path = selected.join(relative);
        }
    }
}

fn discover_stale_profile_root(selected_root: &Path) -> Option<PathBuf> {
    let path = selected_root.join("config/codex/config.toml");
    let raw = std::fs::read_to_string(path).ok()?;
    let value = raw.parse::<toml::Value>().ok()?;
    let catalog = value.get("model_catalog_json")?.as_str()?;
    let catalog_path = Path::new(catalog);
    if catalog_path.file_name()? != OsStr::new("iyw-claw-models.json")
        || catalog_path.parent()?.file_name()? != OsStr::new("codex")
        || catalog_path.parent()?.parent()?.file_name()? != OsStr::new("config")
    {
        return None;
    }
    let root = catalog_path.parent()?.parent()?.parent()?.to_path_buf();
    (!same_path(&root, selected_root)).then_some(root)
}

fn prepare_profile_path_changes(
    selected_root: &Path,
    previous_root: &Path,
) -> Result<Vec<(PathBuf, String, String)>, AgentStorageError> {
    let previous = previous_root.to_string_lossy();
    let selected = selected_root.to_string_lossy();
    let mut changes = Vec::new();
    for relative in REBASED_PROFILE_FILES {
        let path = selected_root.join(relative);
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(AgentStorageError::InvalidConfig(error.to_string())),
        };
        if raw.contains(previous.as_ref()) {
            changes.push((
                path,
                raw.clone(),
                raw.replace(previous.as_ref(), selected.as_ref()),
            ));
        }
    }
    Ok(changes)
}

fn apply_profile_path_changes(
    changes: &[(PathBuf, String, String)],
) -> Result<(), AgentStorageError> {
    for (path, raw, next) in changes {
        crate::acp::provider_overlay::write_if_changed(path, raw, next)
            .map_err(AgentStorageError::InvalidConfig)?;
    }
    Ok(())
}

fn rollback_profile_path_changes(changes: &[(PathBuf, String, String)]) {
    for (path, raw, next) in changes.iter().rev() {
        let _ = crate::acp::provider_overlay::write_if_changed(path, next, raw);
    }
}

async fn record_install_root(conn: &DatabaseConnection, selected_root: &Path) {
    if let Err(error) = crate::db::service::app_metadata_service::upsert_value(
        conn,
        INSTALL_ROOT_METADATA_KEY,
        &selected_root.to_string_lossy(),
    )
    .await
    {
        tracing::warn!(error = %error, "failed to record desktop installation root");
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    let normalize = |path: &Path| {
        path.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    };
    normalize(left) == normalize(right)
}
