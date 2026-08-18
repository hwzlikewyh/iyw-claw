use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

use super::{
    CliPaths, WeComAiError, CLI_VERSION, COMMAND_NAME, INSTALL_TIMEOUT, LOCK_RETRY_INTERVAL,
    PACKAGE_NAME,
};

#[cfg(windows)]
const LAUNCHER_CONTENT: &[u8] = br#"@echo off
if not defined WECOM_CLI_MANAGED_COMMAND goto unavailable
if not exist "%WECOM_CLI_MANAGED_COMMAND%" goto unavailable
@"%WECOM_CLI_MANAGED_COMMAND%" %*
exit /b %errorlevel%
:unavailable
echo Application-managed WeCom CLI runtime is unavailable. 1>&2
exit /b 127
"#;

#[cfg(not(windows))]
const LAUNCHER_CONTENT: &[u8] = br#"#!/bin/sh
command_path="${WECOM_CLI_MANAGED_COMMAND:-}"
if [ -z "$command_path" ] || [ ! -x "$command_path" ]; then
  echo "Application-managed WeCom CLI runtime is unavailable." >&2
  exit 127
fi
exec "$command_path" "$@"
"#;

pub(super) async fn acquire_install_lock(paths: &CliPaths) -> Result<File, WeComAiError> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&paths.lock)
        .map_err(WeComAiError::InstallLock)?;
    let deadline = Instant::now() + INSTALL_TIMEOUT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                tokio::time::sleep(LOCK_RETRY_INTERVAL).await;
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(WeComAiError::InstallLockTimeout);
            }
            Err(std::fs::TryLockError::Error(source)) => {
                return Err(WeComAiError::InstallLock(source));
            }
        }
    }
}

pub(super) fn ensure_launcher(paths: &CliPaths) -> Result<(), WeComAiError> {
    let path = launcher_path(paths);
    let current = std::fs::read(&path).ok();
    if current.as_deref() != Some(LAUNCHER_CONTENT) {
        std::fs::write(&path, LAUNCHER_CONTENT).map_err(|source| WeComAiError::Io {
            stage: "write managed CLI launcher",
            source,
        })?;
    }
    make_launcher_executable(&path)
}

#[cfg(windows)]
fn make_launcher_executable(_path: &Path) -> Result<(), WeComAiError> {
    Ok(())
}

#[cfg(unix)]
fn make_launcher_executable(path: &Path) -> Result<(), WeComAiError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)
        .map_err(|source| WeComAiError::Io {
            stage: "read managed CLI launcher metadata",
            source,
        })?
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(path, permissions).map_err(|source| WeComAiError::Io {
        stage: "make managed CLI launcher executable",
        source,
    })
}

pub(super) fn launcher_path(paths: &CliPaths) -> PathBuf {
    if cfg!(windows) {
        paths.launcher.join(format!("{COMMAND_NAME}.cmd"))
    } else {
        paths.launcher.join(COMMAND_NAME)
    }
}

pub(super) fn recover_interrupted_activation(paths: &CliPaths) -> Result<(), WeComAiError> {
    if paths.prefix.exists() {
        return Ok(());
    }
    let mut candidates = std::fs::read_dir(&paths.staging)
        .map_err(|source| WeComAiError::Io {
            stage: "scan managed CLI activation recovery",
            source,
        })?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("previous-"))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|entry| entry.metadata().and_then(|value| value.modified()).ok());
    let Some(previous) = candidates.pop() else {
        return Ok(());
    };
    if let Some(parent) = paths.prefix.parent() {
        std::fs::create_dir_all(parent).map_err(|source| WeComAiError::Io {
            stage: "create managed CLI recovery destination",
            source,
        })?;
    }
    std::fs::rename(previous.path(), &paths.prefix).map_err(WeComAiError::Activation)
}

pub(super) async fn install_and_activate(
    paths: &CliPaths,
    staging: &Path,
    npm_config: &Path,
) -> Result<(), WeComAiError> {
    tokio::fs::create_dir_all(staging)
        .await
        .map_err(|source| WeComAiError::Io {
            stage: "create CLI install staging directory",
            source,
        })?;
    prepare_npm_config(npm_config).await?;
    run_npm_install(paths, staging, npm_config).await?;
    validate_cli(staging)?;
    activate_staged_cli(paths, staging)
}

async fn prepare_npm_config(directory: &Path) -> Result<(), WeComAiError> {
    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|source| WeComAiError::Io {
            stage: "create isolated npm config directory",
            source,
        })?;
    for name in ["user.npmrc", "global.npmrc"] {
        tokio::fs::write(directory.join(name), b"")
            .await
            .map_err(|source| WeComAiError::Io {
                stage: "write isolated npm config",
                source,
            })?;
    }
    Ok(())
}

async fn run_npm_install(
    paths: &CliPaths,
    staging: &Path,
    npm_config: &Path,
) -> Result<(), WeComAiError> {
    let package = format!("{PACKAGE_NAME}@{CLI_VERSION}");
    let mut args = crate::acp::npm_runtime::private_npm_install_args(
        staging,
        &paths.cache,
        &[package.as_str()],
    )
    .map_err(|_| WeComAiError::InstallConfiguration)?;
    args.push(crate::acp::npm_runtime::path_arg(
        "--userconfig=",
        &npm_config.join("user.npmrc"),
    ));
    args.push(crate::acp::npm_runtime::path_arg(
        "--globalconfig=",
        &npm_config.join("global.npmrc"),
    ));

    let managed_program = crate::acp::version_center::managed_tool_executable("npm");
    let program = managed_program
        .clone()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("npm"));
    let mut command = crate::process::tokio_command(program);
    apply_managed_node_path(&mut command, managed_program.as_deref());
    command
        .args(args)
        .env("NPM_CONFIG_UPDATE_NOTIFIER", "false")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    wait_for_installer(command).await
}

fn apply_managed_node_path(command: &mut tokio::process::Command, npm: Option<&Path>) {
    let Some(directory) = npm.and_then(Path::parent) else {
        return;
    };
    let mut paths = vec![directory.to_path_buf()];
    let existing = std::env::var_os("PATH").unwrap_or_default();
    paths.extend(std::env::split_paths(&existing));
    if let Ok(joined) = std::env::join_paths(paths) {
        command.env("PATH", joined);
    }
}

async fn wait_for_installer(mut command: tokio::process::Command) -> Result<(), WeComAiError> {
    let mut child = command.spawn().map_err(WeComAiError::InstallProcess)?;
    let pid = child.id();
    let status = match tokio::time::timeout(INSTALL_TIMEOUT, child.wait()).await {
        Ok(result) => result.map_err(WeComAiError::InstallProcess)?,
        Err(_) => {
            if let Some(pid) = pid {
                let _ = kill_tree::tokio::kill_tree(pid).await;
            }
            return Err(WeComAiError::InstallTimeout);
        }
    };
    status
        .success()
        .then_some(())
        .ok_or_else(|| WeComAiError::InstallFailed(status.code()))
}

pub(super) fn cli_is_valid(paths: &CliPaths) -> bool {
    validate_cli(&paths.prefix).is_ok()
}

fn validate_cli(prefix: &Path) -> Result<(), WeComAiError> {
    let manifest = package_manifest_path(prefix)
        .ok_or(WeComAiError::Validation("package manifest is missing"))?;
    let content = std::fs::read_to_string(manifest).map_err(|source| WeComAiError::Io {
        stage: "read CLI package manifest",
        source,
    })?;
    let value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|_| WeComAiError::Validation("package manifest is invalid"))?;
    if value.get("version").and_then(|value| value.as_str()) != Some(CLI_VERSION) {
        return Err(WeComAiError::Validation("package version does not match"));
    }
    if !command_path(prefix).is_file() {
        return Err(WeComAiError::Validation("wecom-cli command is missing"));
    }
    Ok(())
}

fn package_manifest_path(prefix: &Path) -> Option<PathBuf> {
    [prefix.join("node_modules"), prefix.join("lib/node_modules")]
        .into_iter()
        .map(|root| root.join("@wecom/cli/package.json"))
        .find(|path| path.is_file())
}

pub(super) fn command_path(prefix: &Path) -> PathBuf {
    let bin = crate::acp::npm_runtime::npm_prefix_bin_dir(prefix);
    if cfg!(windows) {
        bin.join(format!("{COMMAND_NAME}.cmd"))
    } else {
        bin.join(COMMAND_NAME)
    }
}

fn activate_staged_cli(paths: &CliPaths, staging: &Path) -> Result<(), WeComAiError> {
    let parent = paths
        .prefix
        .parent()
        .ok_or(WeComAiError::Validation("CLI destination has no parent"))?;
    std::fs::create_dir_all(parent).map_err(|source| WeComAiError::Io {
        stage: "create CLI destination directory",
        source,
    })?;
    let backup = paths
        .staging
        .join(format!("previous-{}", uuid::Uuid::new_v4()));
    let had_previous = paths.prefix.exists();
    if had_previous {
        std::fs::rename(&paths.prefix, &backup).map_err(WeComAiError::Activation)?;
    }
    if let Err(activation) = std::fs::rename(staging, &paths.prefix) {
        return match had_previous {
            true => match std::fs::rename(&backup, &paths.prefix) {
                Ok(()) => Err(WeComAiError::Activation(activation)),
                Err(rollback) => Err(WeComAiError::ActivationRollback {
                    activation,
                    rollback,
                }),
            },
            false => Err(WeComAiError::Activation(activation)),
        };
    }
    if had_previous {
        let _ = std::fs::remove_dir_all(backup);
    }
    Ok(())
}
