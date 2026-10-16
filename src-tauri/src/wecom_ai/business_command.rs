use std::path::{Path, PathBuf};

use super::{installer, CliPaths, WeComAiError};

/// 业务参数直接传给受管入口，Windows 不经过 .cmd 的二次展开。
pub(crate) fn command(data_dir: &Path) -> Result<tokio::process::Command, WeComAiError> {
    let paths = CliPaths::new(data_dir);
    if !installer::cli_is_valid(&paths) {
        return Err(WeComAiError::Validation("managed CLI is invalid"));
    }
    let entry = entry(&paths.prefix)?;
    let mut command = match entry.extension().and_then(|extension| extension.to_str()) {
        Some("js" | "cjs" | "mjs") => {
            let node = crate::acp::version_center::managed_tool_executable("node")
                .or_else(|| which::which("node").ok())
                .ok_or(WeComAiError::Validation("Node runtime is unavailable"))?;
            let mut command = crate::process::tokio_command(node);
            command.arg(&entry);
            command
        }
        Some("cmd" | "bat" | "ps1") => {
            return Err(WeComAiError::Validation(
                "CLI business entry requires a shell",
            ))
        }
        _ => crate::process::tokio_command(&entry),
    };
    command.current_dir(&paths.config);
    Ok(command)
}

fn entry(prefix: &Path) -> Result<PathBuf, WeComAiError> {
    let root = [
        prefix.join("node_modules/@wecom/cli"),
        prefix.join("lib/node_modules/@wecom/cli"),
    ]
    .into_iter()
    .find(|root| root.join("package.json").is_file())
    .ok_or(WeComAiError::Validation("CLI package manifest is missing"))?;
    let manifest = std::fs::read_to_string(root.join("package.json"))
        .map_err(|_| WeComAiError::Validation("CLI package manifest is unreadable"))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)
        .map_err(|_| WeComAiError::Validation("CLI package manifest is invalid"))?;
    let bin = manifest["bin"]["wecom-cli"]
        .as_str()
        .or_else(|| manifest["bin"].as_str())
        .ok_or(WeComAiError::Validation("CLI business entry is missing"))?;
    let root = root
        .canonicalize()
        .map_err(|_| WeComAiError::Validation("CLI package is unavailable"))?;
    let entry = root
        .join(bin)
        .canonicalize()
        .map_err(|_| WeComAiError::Validation("CLI business entry is unavailable"))?;
    if !entry.starts_with(&root) || !entry.is_file() {
        return Err(WeComAiError::Validation(
            "CLI business entry escaped its package",
        ));
    }
    Ok(entry)
}
