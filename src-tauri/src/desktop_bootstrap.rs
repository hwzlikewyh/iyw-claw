use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use crate::acp::agent_storage::{load_config, save_config, AgentStorageConfig, AgentStorageError};

const APP_DIR_NAME: &str = "app";
const DATA_DIR_ENV: &str = "IYW_CLAW_DATA_DIR";
const HOME_DIR_ENV: &str = "IYW_CLAW_HOME";
const LOG_DIR_ENV: &str = "IYW_CLAW_LOG_DIR";
const USER_MEMORY_APP_DIR_NAME: &str = ".iyw-claw";
pub const INSTALL_ROOT_ENV: &str = "IYW_CLAW_INSTALL_ROOT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopBootstrap {
    selected_root: Option<PathBuf>,
    legacy_home: Option<PathBuf>,
    default_user_memory_root: Option<PathBuf>,
    user_memory_root:
        Result<crate::paths::ResolvedUserMemoryRoot, crate::paths::UserMemoryPathError>,
}

impl DesktopBootstrap {
    pub fn selected_root(&self) -> Option<&Path> {
        self.selected_root.as_deref()
    }

    pub fn legacy_home(&self) -> Option<&Path> {
        self.legacy_home.as_deref()
    }

    pub fn user_memory_root(
        &self,
    ) -> &Result<crate::paths::ResolvedUserMemoryRoot, crate::paths::UserMemoryPathError> {
        &self.user_memory_root
    }

    pub fn user_memory_migration_sources(
        &self,
        effective_data_dir: &Path,
    ) -> Vec<crate::user_memory::UserMemoryMigrationSource> {
        use crate::user_memory::UserMemoryLegacySourceKind as Kind;

        let mut sources = Vec::new();
        push_migration_source(&mut sources, Kind::ConfiguredHome, self.legacy_home.clone());
        push_migration_source(
            &mut sources,
            Kind::DefaultHome,
            self.default_user_memory_root.clone(),
        );
        push_migration_source(
            &mut sources,
            Kind::InstallData,
            self.selected_root.as_ref().map(|root| root.join("data")),
        );
        push_migration_source(
            &mut sources,
            Kind::AppData,
            Some(effective_data_dir.to_path_buf()),
        );
        sources
    }
}

fn push_migration_source(
    sources: &mut Vec<crate::user_memory::UserMemoryMigrationSource>,
    kind: crate::user_memory::UserMemoryLegacySourceKind,
    path: Option<PathBuf>,
) {
    if let Some(path) = path {
        sources.push(crate::user_memory::UserMemoryMigrationSource { kind, path });
    }
}

pub fn initial_agent_storage_root(selected_root: Option<&Path>, _data_dir: &Path) -> PathBuf {
    selected_root
        .map(Path::to_path_buf)
        .unwrap_or_else(crate::paths::iyw_claw_user_dir)
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
        .or_else(|| Some(crate::paths::iyw_claw_user_dir().join("data")))
}

pub fn apply_pre_runtime_environment() -> DesktopBootstrap {
    let legacy_home = std::env::var_os(HOME_DIR_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(absolutize);
    let user_memory_root = crate::paths::desktop_user_memory_root();
    let default_user_memory_root = dirs::home_dir()
        .map(|home| home.join(USER_MEMORY_APP_DIR_NAME))
        .map(absolutize);
    let install_root = std::env::current_exe()
        .ok()
        .and_then(|executable| resolve_install_root(&executable));
    let data_root = resolve_data_root(std::env::var_os(DATA_DIR_ENV), install_root.as_deref());

    if let Some(data_root) = data_root.as_deref() {
        std::env::set_var(DATA_DIR_ENV, data_root);
    }
    let installed_data_root = data_root.as_deref().filter(|data| {
        install_root
            .as_ref()
            .is_some_and(|root| root.join("data") == *data)
    });
    let asset_root = legacy_home
        .as_deref()
        .or(installed_data_root)
        .map(Path::to_path_buf)
        .unwrap_or_else(crate::paths::iyw_claw_user_dir);
    std::env::set_var(HOME_DIR_ENV, asset_root);
    // Agent 路径由数据库的已保存配置解析，不能在加载配置前覆盖为新默认值。
    if std::env::var_os(LOG_DIR_ENV).is_none_or(|value| value.is_empty()) {
        let log_root = install_root
            .clone()
            .unwrap_or_else(crate::paths::iyw_claw_user_dir)
            .join("logs");
        std::env::set_var(LOG_DIR_ENV, log_root);
    }
    if let Some(root) = install_root.as_deref() {
        std::env::set_var(INSTALL_ROOT_ENV, root);
    }

    DesktopBootstrap {
        selected_root: install_root,
        legacy_home,
        default_user_memory_root,
        user_memory_root,
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

/// 新默认目录只用于未配置的安装；升级保留已确认的会话与 profile 路径。
pub async fn reconcile_agent_storage_root(
    conn: &DatabaseConnection,
    selected_root: &Path,
) -> Result<Option<PathBuf>, AgentStorageError> {
    let Some(mut config) = load_config(conn).await? else {
        return Ok(None);
    };
    if config.initialized
        && config
            .root
            .as_ref()
            .is_some_and(|root| !root.as_os_str().is_empty())
    {
        return Ok(None);
    }
    let previous = config.root.replace(selected_root.to_path_buf());
    config.initialized = true;
    save_config(conn, &config).await?;
    Ok(previous)
}

fn absolutize(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from(OsStr::new(".")))
        .join(path)
}
