use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::Mutex;

mod environment;
mod installer;
pub use environment::{
    inherit_terminal_environment, inject_runtime_environment, reapply_runtime_path,
};

pub const CLI_VERSION: &str = "1.1.0";
pub const CONFIG_DIR_ENV: &str = "WECOM_CLI_CONFIG_DIR";
pub const MANAGED_COMMAND_ENV: &str = "WECOM_CLI_MANAGED_COMMAND";

pub(super) const PACKAGE_NAME: &str = "@wecom/cli";
pub(super) const COMMAND_NAME: &str = "wecom-cli";
pub(super) const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);
pub(super) const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, thiserror::Error)]
pub enum WeComAiError {
    #[error("{stage} failed: {source}")]
    Io {
        stage: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("managed CLI install lock timed out")]
    InstallLockTimeout,
    #[error("managed CLI install lock failed: {0}")]
    InstallLock(std::io::Error),
    #[error("managed npm installation could not be prepared")]
    InstallConfiguration,
    #[error("managed npm installation process failed: {0}")]
    InstallProcess(std::io::Error),
    #[error("managed npm installation timed out")]
    InstallTimeout,
    #[error("managed npm installation exited unsuccessfully (code: {0:?})")]
    InstallFailed(Option<i32>),
    #[error("managed CLI validation failed: {0}")]
    Validation(&'static str),
    #[error("managed CLI activation failed: {0}")]
    Activation(std::io::Error),
    #[error(
        "managed CLI activation and rollback failed: activation={activation}; rollback={rollback}"
    )]
    ActivationRollback {
        activation: std::io::Error,
        rollback: std::io::Error,
    },
}

pub(super) struct CliPaths {
    pub(super) prefix: PathBuf,
    pub(super) command: PathBuf,
    pub(super) launcher: PathBuf,
    pub(super) config: PathBuf,
    pub(super) cache: PathBuf,
    pub(super) staging: PathBuf,
    pub(super) lock: PathBuf,
}

impl CliPaths {
    pub(super) fn new(data_dir: &Path) -> Self {
        let root = runtime_root(data_dir);
        let prefix = prefix_from_root(&root);
        Self {
            command: installer::command_path(&prefix),
            prefix,
            launcher: root.join("bin"),
            config: root.join("config"),
            cache: root.join("cache"),
            staging: root.join("staging"),
            lock: root.join("install.lock"),
        }
    }
}

fn install_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub async fn ensure_cli_best_effort(data_dir: &Path, trigger: &'static str) -> bool {
    match ensure_cli(data_dir).await {
        Ok(installed) => {
            tracing::info!(
                trigger,
                version = CLI_VERSION,
                installed,
                "[wecom-ai] managed CLI ready"
            );
            true
        }
        Err(error) => {
            tracing::warn!(
                trigger,
                version = CLI_VERSION,
                runtime_root = %runtime_root(data_dir).display(),
                error = %error,
                "[wecom-ai] managed CLI unavailable"
            );
            false
        }
    }
}

pub async fn ensure_cli(data_dir: &Path) -> Result<bool, WeComAiError> {
    let _guard = install_lock().lock().await;
    let paths = CliPaths::new(data_dir);
    create_runtime_directories(&paths).await?;
    let _file_lock = installer::acquire_install_lock(&paths).await?;
    if migrate_legacy_config_if_empty(&paths.config).map_err(|source| WeComAiError::Io {
        stage: "migrate legacy CLI configuration",
        source,
    })? {
        tracing::info!("[wecom-ai] migrated legacy CLI configuration");
    }
    installer::ensure_launcher(&paths)?;
    installer::recover_interrupted_activation(&paths)?;
    if installer::cli_is_valid(&paths) {
        return Ok(false);
    }

    let nonce = uuid::Uuid::new_v4();
    let staging = paths.staging.join(format!("install-{nonce}"));
    let npm_config = paths.staging.join(format!("npm-config-{nonce}"));
    let result = installer::install_and_activate(&paths, &staging, &npm_config).await;
    let _ = tokio::fs::remove_dir_all(&npm_config).await;
    if result.is_err() {
        let _ = tokio::fs::remove_dir_all(&staging).await;
    }
    result.map(|()| true)
}

fn migrate_legacy_config_if_empty(target: &Path) -> Result<bool, std::io::Error> {
    if std::fs::read_dir(target)?.next().is_some() {
        return Ok(false);
    }
    let Some(source) = dirs::home_dir().map(|home| home.join(".config").join("wecom")) else {
        return Ok(false);
    };
    if !source.is_dir() || source == target {
        return Ok(false);
    }
    let staging = target.with_extension(format!("import-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&staging)?;
    if let Err(error) = copy_config_entries(&source, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    std::fs::remove_dir(target)?;
    if let Err(error) = std::fs::rename(&staging, target) {
        let _ = std::fs::create_dir_all(target);
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    Ok(true)
}

fn copy_config_entries(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            std::fs::create_dir_all(&destination)?;
            copy_config_entries(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

pub fn cli_is_ready(data_dir: &Path) -> bool {
    let paths = CliPaths::new(data_dir);
    installer::launcher_path(&paths).is_file() && installer::cli_is_valid(&paths)
}

pub fn managed_command(data_dir: &Path) -> Result<tokio::process::Command, WeComAiError> {
    let paths = CliPaths::new(data_dir);
    if !installer::launcher_path(&paths).is_file() {
        return Err(WeComAiError::Validation("managed launcher is missing"));
    }
    if !installer::cli_is_valid(&paths) {
        return Err(WeComAiError::Validation("managed CLI is invalid"));
    }
    let mut command = crate::process::tokio_command(installer::launcher_path(&paths));
    command.env(CONFIG_DIR_ENV, &paths.config);
    command.env(MANAGED_COMMAND_ENV, &paths.command);
    Ok(command)
}

async fn create_runtime_directories(paths: &CliPaths) -> Result<(), WeComAiError> {
    for (stage, path) in [
        ("create CLI launcher directory", &paths.launcher),
        ("create CLI config directory", &paths.config),
        ("create CLI cache directory", &paths.cache),
        ("create CLI staging directory", &paths.staging),
    ] {
        tokio::fs::create_dir_all(path)
            .await
            .map_err(|source| WeComAiError::Io { stage, source })?;
    }
    Ok(())
}

pub(super) fn prefix_from_root(root: &Path) -> PathBuf {
    root.join("cli")
        .join(CLI_VERSION)
        .join(crate::acp::registry::current_platform())
}

pub fn runtime_root(data_dir: &Path) -> PathBuf {
    data_dir.join("wecom-ai")
}
