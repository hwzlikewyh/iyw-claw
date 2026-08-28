use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::{Child, ChildStderr};

use super::types::{PluginInvokeError, RuntimeLaunchSpec};

pub(super) struct SpawnedPluginProcess {
    pub child: Child,
    pub stderr: ChildStderr,
}

pub(super) fn spawn(spec: &RuntimeLaunchSpec) -> Result<SpawnedPluginProcess, PluginInvokeError> {
    let root = canonical_directory(&spec.install_root, "plugin version")?;
    let entrypoint = canonical_file(&root.join(&spec.entrypoint), &root, "entrypoint")?;
    let program = runtime_program(&spec.runtime_kind, &entrypoint)?;
    std::fs::create_dir_all(&spec.plugin_data_dir).map_err(|error| {
        PluginInvokeError::before_effect("plugin_data_unavailable", error.to_string())
    })?;
    let workspace = canonical_directory(&spec.workspace_dir, "workspace")?;
    let mut command = crate::process::tokio_command(&program);
    command.env_clear();
    apply_minimal_environment(&mut command, &root, &spec.plugin_data_dir, &workspace);
    command.current_dir(&workspace);
    if spec.runtime_kind != "binary" {
        command.arg(&entrypoint);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| {
        PluginInvokeError::before_effect("plugin_start_failed", error.to_string())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        PluginInvokeError::before_effect("plugin_start_failed", "stderr pipe is unavailable")
    })?;
    Ok(SpawnedPluginProcess { child, stderr })
}

fn runtime_program(kind: &str, entrypoint: &Path) -> Result<PathBuf, PluginInvokeError> {
    if kind == "binary"
        && entrypoint.extension().is_some_and(|extension| {
            matches!(
                extension.to_string_lossy().to_ascii_lowercase().as_str(),
                "cmd" | "bat" | "ps1" | "sh"
            )
        })
    {
        return Err(PluginInvokeError::before_effect(
            "plugin_runtime_unavailable",
            "Script entrypoints are not allowed for binary runtimes",
        ));
    }
    let path = match kind {
        "node" => crate::acp::version_center::managed_tool_executable("node"),
        "python" => crate::acp::version_center::managed_tool_executable("python"),
        "binary" => Some(entrypoint.to_path_buf()),
        _ => None,
    }
    .ok_or_else(|| {
        PluginInvokeError::before_effect("plugin_runtime_unavailable", "managed runtime missing")
    })?;
    canonical_file(&path, path.parent().unwrap_or(Path::new(".")), "runtime")
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, PluginInvokeError> {
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        PluginInvokeError::before_effect("plugin_path_invalid", format!("{label}: {error}"))
    })?;
    if !canonical.is_dir() {
        return Err(PluginInvokeError::before_effect(
            "plugin_path_invalid",
            format!("{label} is not a directory"),
        ));
    }
    Ok(canonical)
}

fn canonical_file(path: &Path, root: &Path, label: &str) -> Result<PathBuf, PluginInvokeError> {
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        PluginInvokeError::before_effect("plugin_path_invalid", format!("{label}: {error}"))
    })?;
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    if !canonical.is_file() || !canonical.starts_with(&root) {
        return Err(PluginInvokeError::before_effect(
            "plugin_path_invalid",
            format!("{label} escapes its trusted root"),
        ));
    }
    Ok(canonical)
}

fn apply_minimal_environment(
    command: &mut tokio::process::Command,
    plugin_root: &Path,
    data_root: &Path,
    workspace: &Path,
) {
    command
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .env("IYW_PLUGIN_ROOT", plugin_root)
        .env("IYW_PLUGIN_DATA_DIR", data_root)
        .env("IYW_WORKSPACE_DIR", workspace);
    for key in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}
