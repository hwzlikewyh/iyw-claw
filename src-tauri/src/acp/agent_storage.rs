use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::error::DbError;
use crate::db::service::app_metadata_service;
use crate::models::agent::AgentType;

pub const STORAGE_ROOT_ENV: &str = "IYW_CLAW_AGENT_STORAGE_DIR";
pub const STORAGE_METADATA_KEY: &str = "agent_storage.config.v1";

#[derive(Debug, Error)]
pub enum AgentStorageError {
    #[error(transparent)]
    Database(#[from] DbError),
    #[error("invalid agent storage config: {0}")]
    InvalidConfig(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentStorageConfig {
    pub root: Option<PathBuf>,
    pub initialized: bool,
    pub allow_system_drive: bool,
    #[serde(default)]
    pub import_version: u32,
    #[serde(default)]
    pub profile_overrides: BTreeMap<String, PathBuf>,
}

impl AgentStorageConfig {
    pub fn confirmed(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            initialized: true,
            allow_system_drive: false,
            import_version: 0,
            profile_overrides: BTreeMap::new(),
        }
    }
}

pub async fn load_config(
    conn: &DatabaseConnection,
) -> Result<Option<AgentStorageConfig>, AgentStorageError> {
    let Some(raw) = app_metadata_service::get_value(conn, STORAGE_METADATA_KEY).await? else {
        return Ok(None);
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|error| AgentStorageError::InvalidConfig(error.to_string()))
}

pub async fn save_config(
    conn: &DatabaseConnection,
    config: &AgentStorageConfig,
) -> Result<(), AgentStorageError> {
    let raw = serde_json::to_string(config)
        .map_err(|error| AgentStorageError::InvalidConfig(error.to_string()))?;
    app_metadata_service::upsert_value(conn, STORAGE_METADATA_KEY, &raw).await?;
    Ok(())
}

pub fn resolve_root(
    env_override: Option<OsString>,
    persisted: Option<&AgentStorageConfig>,
    server_fallback: Option<PathBuf>,
) -> Option<PathBuf> {
    env_override
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            persisted
                .filter(|config| config.initialized)
                .and_then(|config| config.root.clone())
        })
        .or(server_fallback)
}

pub fn suggest_desktop_root(executable_path: &Path) -> Option<PathBuf> {
    let install_dir = executable_path.parent()?;
    if install_dir.file_name() == Some(std::ffi::OsStr::new("app")) {
        return install_dir.parent().map(Path::to_path_buf);
    }
    if install_dir.file_name() == Some(std::ffi::OsStr::new("iyw-claw")) {
        return Some(install_dir.to_path_buf());
    }
    Some(install_dir.join("iyw-claw"))
}

pub use crate::acp::agent_profile::{
    activate_startup_profile_env, effective_startup_config, startup_profile_env,
    startup_profile_env_is_complete, startup_profile_env_matches,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStoragePaths {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfilePaths {
    pub root: PathBuf,
    pub env: BTreeMap<&'static str, PathBuf>,
}

impl AgentStoragePaths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn active() -> Option<Self> {
        resolve_root(std::env::var_os(STORAGE_ROOT_ENV), None, None).map(Self::new)
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn runtime_dir(&self) -> PathBuf {
        self.root.join("runtime")
    }

    pub fn binary_runtime_dir(&self) -> PathBuf {
        self.runtime_dir().join("binary")
    }

    pub fn npm_runtime_dir(&self) -> PathBuf {
        self.runtime_dir().join("npm")
    }

    pub fn npm_cache_dir(&self) -> PathBuf {
        self.npm_runtime_dir().join("cache")
    }

    pub fn uv_runtime_dir(&self) -> PathBuf {
        self.runtime_dir().join("uv")
    }

    pub fn uv_cache_dir(&self) -> PathBuf {
        self.uv_runtime_dir().join("cache")
    }

    pub fn config_dir(&self) -> PathBuf {
        self.root.join("config")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.runtime_dir().join("downloads")
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.runtime_dir().join("staging")
    }

    pub fn trash_dir(&self) -> PathBuf {
        self.runtime_dir().join("trash")
    }

    pub fn profile(&self, agent_type: AgentType) -> AgentProfilePaths {
        let config_dir = self.config_dir();
        let (root, env) = match agent_type {
            AgentType::ClaudeCode => {
                single_profile_env(config_dir.join("claude"), "CLAUDE_CONFIG_DIR")
            }
            AgentType::Codex => single_profile_env(config_dir.join("codex"), "CODEX_HOME"),
            AgentType::Gemini => {
                let home = config_dir.join("gemini-home");
                let mut env = BTreeMap::new();
                env.insert("GEMINI_CLI_HOME", home.clone());
                (home.join(".gemini"), env)
            }
            AgentType::OpenClaw => {
                let root = config_dir.join("openclaw");
                let mut env = BTreeMap::new();
                env.insert("OPENCLAW_HOME", config_dir.join("openclaw-home"));
                env.insert("OPENCLAW_STATE_DIR", root.clone());
                (root, env)
            }
            AgentType::OpenCode => {
                let root = config_dir.join("opencode");
                let mut env = BTreeMap::new();
                env.insert("XDG_CONFIG_HOME", root.join("config"));
                env.insert("XDG_DATA_HOME", root.join("data"));
                env.insert("XDG_CACHE_HOME", root.join("cache"));
                (root, env)
            }
            AgentType::Cline => single_profile_env(config_dir.join("cline"), "CLINE_DIR"),
            AgentType::Hermes => single_profile_env(config_dir.join("hermes"), "HERMES_HOME"),
            AgentType::CodeBuddy => {
                single_profile_env(config_dir.join("codebuddy"), "CODEBUDDY_CONFIG_DIR")
            }
            AgentType::KimiCode => {
                single_profile_env(config_dir.join("kimi-code"), "KIMI_CODE_HOME")
            }
            AgentType::Pi => single_profile_env(config_dir.join("pi"), "PI_CODING_AGENT_DIR"),
        };
        AgentProfilePaths { root, env }
    }
}

fn single_profile_env(
    root: PathBuf,
    env_key: &'static str,
) -> (PathBuf, BTreeMap<&'static str, PathBuf>) {
    let mut env = BTreeMap::new();
    env.insert(env_key, root.clone());
    (root, env)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RootValidation {
    pub absolute_path: PathBuf,
    pub writable: bool,
    pub on_system_drive: bool,
    pub error: Option<String>,
}

pub fn validate_root(path: &Path, system_drive: Option<&str>) -> RootValidation {
    let on_system_drive = is_windows_system_drive(path, system_drive);
    if !path.is_absolute() {
        return invalid_validation(path, on_system_drive, "path must be absolute");
    }
    if path.exists() && !path.is_dir() {
        return invalid_validation(path, on_system_drive, "path must be a directory");
    }
    if let Err(error) = std::fs::create_dir_all(path) {
        return invalid_validation(
            path,
            on_system_drive,
            &format!("create directory failed: {error}"),
        );
    }
    match probe_writable(path) {
        Ok(()) => RootValidation {
            absolute_path: path.to_path_buf(),
            writable: true,
            on_system_drive,
            error: None,
        },
        Err(error) => invalid_validation(path, on_system_drive, &error),
    }
}

fn invalid_validation(path: &Path, on_system_drive: bool, error: &str) -> RootValidation {
    RootValidation {
        absolute_path: path.to_path_buf(),
        writable: false,
        on_system_drive,
        error: Some(error.to_string()),
    }
}

fn probe_writable(path: &Path) -> Result<(), String> {
    let probe_path = path.join(format!(".iyw-claw-write-probe-{}", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path)
        .map_err(|error| format!("write probe failed: {error}"))?;
    file.write_all(b"iyw-claw")
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("write probe failed: {error}"))?;
    drop(file);
    std::fs::remove_file(&probe_path).map_err(|error| format!("remove write probe failed: {error}"))
}

pub fn is_windows_system_drive(path: &Path, system_drive: Option<&str>) -> bool {
    let Some(drive) = system_drive
        .map(str::trim)
        .filter(|drive| !drive.is_empty())
    else {
        return false;
    };
    let drive = drive.trim_end_matches(['/', '\\']).to_ascii_lowercase();
    let path = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    path == drive || path.starts_with(&format!("{drive}/"))
}

#[cfg(test)]
#[path = "agent_storage_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "agent_storage_startup_tests.rs"]
mod startup_tests;
