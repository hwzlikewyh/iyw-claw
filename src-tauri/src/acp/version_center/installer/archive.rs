use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use crate::app_error::AppCommandError;

const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_COMPRESSION_RATIO: u64 = 100;
const MAX_SINGLE_FILE_BYTES: u64 = 512 * 1024 * 1024;

pub fn extract_tool_zip(
    bytes: &[u8],
    destination: &Path,
    tool_id: &str,
) -> Result<(), AppCommandError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        AppCommandError::invalid_input("Managed tool archive is not a ZIP")
            .with_detail(error.to_string())
    })?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(AppCommandError::invalid_input(
            "Managed tool archive has too many entries",
        ));
    }
    std::fs::create_dir_all(destination).map_err(AppCommandError::io)?;
    let mut extracted = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            AppCommandError::invalid_input("Managed tool archive entry is unreadable")
                .with_detail(error.to_string())
        })?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(AppCommandError::invalid_input(
                "Managed tool archive contains an unsafe path",
            ));
        };
        if relative.as_os_str().len() > 240 || has_unsafe_link_mode(entry.unix_mode()) {
            return Err(AppCommandError::invalid_input(
                "Managed tool archive contains an unsupported entry",
            ));
        }
        let compressed = entry.compressed_size();
        let declared = entry.size();
        if declared > MAX_SINGLE_FILE_BYTES {
            return Err(AppCommandError::invalid_input(
                "Managed tool archive contains an oversized file",
            ));
        }
        if declared > 0 && (compressed == 0 || declared / compressed.max(1) > MAX_COMPRESSION_RATIO)
        {
            return Err(AppCommandError::invalid_input(
                "Managed tool archive compression ratio is unsafe",
            ));
        }
        let output = destination.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&output).map_err(AppCommandError::io)?;
            continue;
        }
        let parent = output.parent().ok_or_else(|| {
            AppCommandError::invalid_input("Managed tool archive entry has no parent")
        })?;
        std::fs::create_dir_all(parent).map_err(AppCommandError::io)?;
        let remaining = MAX_EXTRACTED_BYTES.saturating_sub(extracted);
        let mut output_file = std::fs::File::create(&output).map_err(AppCommandError::io)?;
        let written = std::io::copy(&mut entry.by_ref().take(remaining + 1), &mut output_file)
            .map_err(AppCommandError::io)?;
        if written > remaining {
            return Err(AppCommandError::invalid_input(
                "Managed tool archive expands beyond the allowed size",
            ));
        }
        extracted += written;
    }
    ensure_tool_layout(&locate_payload(destination, tool_id)?, tool_id)
}

pub fn locate_payload(root: &Path, tool_id: &str) -> Result<PathBuf, AppCommandError> {
    if has_tool_layout(root, tool_id) {
        return Ok(root.to_path_buf());
    }
    let children = std::fs::read_dir(root)
        .map_err(AppCommandError::io)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .collect::<Vec<_>>();
    if children.len() == 1 && has_tool_layout(&children[0].path(), tool_id) {
        return Ok(children[0].path());
    }
    Err(AppCommandError::invalid_input(
        "Managed tool archive layout does not match its capability",
    ))
}

pub async fn probe_payload(
    root: &Path,
    tool_id: &str,
    version: &str,
) -> Result<(), AppCommandError> {
    // Only the tool's own executable reports the managed version. Bundled companions
    // (npm, uvx) carry independent version numbers, so they are probed for successful
    // execution only -- matching them against the tool version always fails.
    let (versioned, companions): (Vec<PathBuf>, Vec<PathBuf>) = match tool_id {
        "git" => (vec![root.join(git_relative_path())], Vec::new()),
        "node" => (
            vec![root.join(node_relative_path())],
            vec![root.join(npm_relative_path())],
        ),
        "uv" => (
            vec![root.join(uv_relative_path("uv"))],
            vec![root.join(uv_relative_path("uvx"))],
        ),
        _ => return Err(AppCommandError::invalid_input("Unknown managed tool")),
    };
    let version_core = version.split('+').next().unwrap_or(version);
    let bin_dir = match tool_id {
        "node" if cfg!(windows) => Some(root.to_path_buf()),
        "node" => Some(root.join("bin")),
        _ => None,
    };
    for command in versioned {
        let text = probe_version_output(&command, bin_dir.as_deref()).await?;
        if !text.contains(version_core) {
            return Err(AppCommandError::invalid_input(
                "Managed tool probe returned an unexpected version",
            ));
        }
    }
    for command in companions {
        probe_version_output(&command, bin_dir.as_deref()).await?;
    }
    Ok(())
}

async fn probe_version_output(
    command: &Path,
    bin_dir: Option<&Path>,
) -> Result<String, AppCommandError> {
    let mut process = probe_command(command);
    process.arg("--version");
    if let Some(bin_dir) = bin_dir {
        let path = std::env::join_paths(
            std::iter::once(bin_dir.to_path_buf()).chain(
                std::env::var_os("PATH")
                    .as_deref()
                    .map(std::env::split_paths)
                    .into_iter()
                    .flatten(),
            ),
        )
        .map_err(|error| AppCommandError::invalid_input(error.to_string()))?;
        process.env("PATH", path);
    }
    let output = process.output().await.map_err(|error| {
        AppCommandError::task_execution_failed("Managed tool probe failed")
            .with_detail(error.to_string())
    })?;
    if !output.status.success() {
        return Err(AppCommandError::invalid_input(
            "Managed tool probe returned an unexpected version",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(not(windows))]
fn probe_command(command: &Path) -> tokio::process::Command {
    crate::process::tokio_command(command)
}

#[cfg(windows)]
fn probe_command(command: &Path) -> tokio::process::Command {
    if command
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cmd"))
    {
        let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
        let mut process = crate::process::tokio_command(shell);
        process
            .args(["/D", "/S", "/C", "\"%IYW_CLAW_PROBE_COMMAND%\""])
            .env("IYW_CLAW_PROBE_COMMAND", command);
        return process;
    }
    crate::process::tokio_command(command)
}

fn has_unsafe_link_mode(mode: Option<u32>) -> bool {
    mode.is_some_and(|mode| (mode & 0o170000) == 0o120000)
}

fn has_tool_layout(root: &Path, tool_id: &str) -> bool {
    match tool_id {
        "git" => root.join(git_relative_path()).is_file(),
        "node" => {
            root.join(node_relative_path()).is_file() && root.join(npm_relative_path()).is_file()
        }
        "uv" => {
            root.join(uv_relative_path("uv")).is_file()
                && root.join(uv_relative_path("uvx")).is_file()
        }
        _ => false,
    }
}

fn git_relative_path() -> PathBuf {
    if cfg!(windows) {
        Path::new("cmd").join("git.exe")
    } else {
        Path::new("bin").join("git")
    }
}

fn node_relative_path() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("node.exe")
    } else {
        Path::new("bin").join("node")
    }
}

fn npm_relative_path() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("npm.cmd")
    } else {
        Path::new("bin").join("npm")
    }
}

fn uv_relative_path(name: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("{name}.exe"))
    } else {
        PathBuf::from(name)
    }
}

fn ensure_tool_layout(root: &Path, tool_id: &str) -> Result<(), AppCommandError> {
    has_tool_layout(root, tool_id)
        .then_some(())
        .ok_or_else(|| AppCommandError::invalid_input("Managed tool files are incomplete"))
}
