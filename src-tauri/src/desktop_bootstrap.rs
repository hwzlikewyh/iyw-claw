use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use crate::acp::agent_storage::{load_config, save_config, AgentStorageConfig, AgentStorageError};

const APP_DIR_NAME: &str = "app";
const DATA_DIR_ENV: &str = "IYW_CLAW_DATA_DIR";
const HOME_DIR_ENV: &str = "IYW_CLAW_HOME";
const LOG_DIR_ENV: &str = "IYW_CLAW_LOG_DIR";
pub const INSTALL_ROOT_ENV: &str = "IYW_CLAW_INSTALL_ROOT";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopBootstrap {
    selected_root: Option<PathBuf>,
}

impl DesktopBootstrap {
    pub fn selected_root(&self) -> Option<&Path> {
        self.selected_root.as_deref()
    }
}

pub fn initial_agent_storage_root(selected_root: Option<&Path>, data_dir: &Path) -> PathBuf {
    selected_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| data_dir.join("agents"))
}

pub fn resolve_install_root(executable: &Path) -> Option<PathBuf> {
    let app_dir = executable.parent()?;
    if app_dir.file_name()? != OsStr::new(APP_DIR_NAME) {
        return None;
    }
    app_dir.parent().map(Path::to_path_buf)
}

pub fn resolve_data_root(
    explicit: Option<OsString>,
    install_root: Option<&Path>,
) -> Option<PathBuf> {
    explicit
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(absolutize)
        .or_else(|| install_root.map(|root| root.join("data")))
}

pub fn apply_pre_runtime_environment() -> DesktopBootstrap {
    let install_root = std::env::current_exe()
        .ok()
        .and_then(|executable| resolve_install_root(&executable));
    let data_root = resolve_data_root(std::env::var_os(DATA_DIR_ENV), install_root.as_deref());

    if let Some(data_root) = data_root.as_deref() {
        std::env::set_var(DATA_DIR_ENV, data_root);
    }
    if let Some(root) = install_root.as_deref() {
        let data_root = data_root
            .as_deref()
            .unwrap_or_else(|| unreachable!("installed desktop always has a data root"));
        std::env::set_var(HOME_DIR_ENV, data_root);
        std::env::set_var(LOG_DIR_ENV, root.join("logs"));
        std::env::set_var(INSTALL_ROOT_ENV, root);
    }

    DesktopBootstrap {
        selected_root: install_root,
    }
}

pub async fn ensure_initial_agent_storage(
    conn: &DatabaseConnection,
    selected_root: &Path,
) -> Result<(), AgentStorageError> {
    if load_config(conn).await?.is_none() {
        save_config(
            conn,
            &AgentStorageConfig::confirmed(selected_root.to_path_buf()),
        )
        .await?;
    }
    Ok(())
}

fn absolutize(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from(OsStr::new(".")))
        .join(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_agent_storage_root_prefers_portable_install_root() {
        let install_root = Path::new("/opt/iyw-claw");
        let data_dir = Path::new("/Users/me/Library/Application Support/iyw-claw");

        assert_eq!(
            initial_agent_storage_root(Some(install_root), data_dir),
            install_root
        );
    }

    #[test]
    fn initial_agent_storage_root_uses_app_data_for_macos_bundle() {
        let data_dir = Path::new("/Users/me/Library/Application Support/iyw-claw");

        assert_eq!(
            initial_agent_storage_root(None, data_dir),
            data_dir.join("agents")
        );
    }
}
