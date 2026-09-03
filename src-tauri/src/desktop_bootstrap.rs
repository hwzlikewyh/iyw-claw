use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use crate::acp::agent_storage::{load_config, save_config, AgentStorageConfig, AgentStorageError};

const APP_DIR_NAME: &str = "app";
const DATA_DIR_ENV: &str = "IYW_CLAW_DATA_DIR";
const HOME_DIR_ENV: &str = "IYW_CLAW_HOME";
const LOG_DIR_ENV: &str = "IYW_CLAW_LOG_DIR";
const USER_MEMORY_APP_DIR_NAME: &str = ".iyw-claw";
const INSTALL_ROOT_METADATA_KEY: &str = "desktop.install_root.v1";
pub const INSTALL_ROOT_ENV: &str = "IYW_CLAW_INSTALL_ROOT";
const REBASED_PROFILE_FILES: [&str; 2] = ["config/codex/config.toml", "config/hermes/config.yaml"];

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
    if let Some(root) = install_root.as_deref() {
        let data_root = data_root
            .as_deref()
            .unwrap_or_else(|| unreachable!("installed desktop always has a data root"));
        std::env::set_var(HOME_DIR_ENV, data_root);
        if std::env::var_os(LOG_DIR_ENV).is_none_or(|value| value.is_empty()) {
            std::env::set_var(LOG_DIR_ENV, root.join("logs"));
        }
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

/// Rebase a persisted Agent storage root onto the current installation root.
///
/// Older installs could persist a former installation root after an upgrade.
/// A custom Agent-only directory remains user-controlled: rebasing requires a
/// recorded prior install root, a complete legacy install layout, or an active
/// managed profile path that still names the old root.
pub async fn reconcile_agent_storage_root(
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
    stale_roots_from_codex(selected_root)
        .into_iter()
        .chain(stale_root_from_hermes(selected_root))
        .find(|root| !same_path(root, selected_root))
}

fn stale_roots_from_codex(selected_root: &Path) -> Vec<PathBuf> {
    let path = selected_root.join("config/codex/config.toml");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = raw.parse::<toml::Value>() else {
        return Vec::new();
    };
    let catalog_root = value
        .get("model_catalog_json")
        .and_then(toml::Value::as_str)
        .and_then(root_from_catalog_path);
    let command_root = value
        .get("mcp_servers")
        .and_then(|table| table.get("open-computer-use"))
        .and_then(|table| table.get("command"))
        .and_then(toml::Value::as_str)
        .and_then(root_from_managed_tool_path);
    [command_root, catalog_root].into_iter().flatten().collect()
}

fn stale_root_from_hermes(selected_root: &Path) -> Option<PathBuf> {
    let path = selected_root.join("config/hermes/config.yaml");
    let raw = std::fs::read_to_string(path).ok()?;
    let value = serde_yaml::from_str::<serde_yaml::Value>(&raw).ok()?;
    let command = yaml_value(&value, "mcp_servers")
        .and_then(|value| yaml_value(value, "open-computer-use"))
        .and_then(|value| yaml_value(value, "command"))
        .and_then(serde_yaml::Value::as_str)?;
    root_from_managed_tool_path(command)
}

fn yaml_value<'a>(value: &'a serde_yaml::Value, key: &str) -> Option<&'a serde_yaml::Value> {
    value
        .as_mapping()?
        .get(serde_yaml::Value::String(key.to_string()))
}

fn root_from_catalog_path(value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    if path.file_name()? != OsStr::new("iyw-claw-models.json")
        || path.parent()?.file_name()? != OsStr::new("codex")
        || path.parent()?.parent()?.file_name()? != OsStr::new("config")
    {
        return None;
    }
    path.parent()?.parent()?.parent().map(Path::to_path_buf)
}

fn root_from_managed_tool_path(value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    if !path.is_absolute() {
        return None;
    }
    let components: Vec<_> = path.components().collect();
    let runtime_index = components.windows(2).position(|pair| {
        pair[0].as_os_str() == OsStr::new("runtime") && pair[1].as_os_str() == OsStr::new("npm")
    })?;
    if runtime_index == 0 {
        return None;
    }
    let mut root = PathBuf::new();
    for component in &components[..runtime_index] {
        root.push(component.as_os_str());
    }
    Some(root)
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
        if !raw.contains(previous.as_ref()) {
            continue;
        }
        let next = raw.replace(previous.as_ref(), selected.as_ref());
        changes.push((path, raw, next));
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

fn absolutize(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from(OsStr::new(".")))
        .join(path)
}
