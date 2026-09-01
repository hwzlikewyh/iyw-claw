use std::ffi::OsStr;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Output;

use crate::app_error::AppCommandError;

const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_COMPRESSION_RATIO: u64 = 100;
const MAX_SINGLE_FILE_BYTES: u64 = 512 * 1024 * 1024;
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

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
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&output, std::fs::Permissions::from_mode(mode & 0o777))
                .map_err(AppCommandError::io)?;
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
        "browser-engine" => (vec![root.join(browser_engine_relative_path())], Vec::new()),
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
        if tool_id != "browser-engine" && !text.contains(version_core) {
            return Err(unexpected_version(tool_id, version_core, &text));
        }
        if tool_id == "browser-engine" && text.trim().is_empty() {
            return Err(AppCommandError::task_execution_failed(
                "Managed browser engine probe returned no version",
            ));
        }
    }
    if tool_id == "node" {
        probe_npm_version(root, bin_dir.as_deref()).await?;
    } else {
        for command in companions {
            probe_version_output(&command, bin_dir.as_deref()).await?;
        }
    }
    Ok(())
}

async fn probe_npm_version(root: &Path, bin_dir: Option<&Path>) -> Result<(), AppCommandError> {
    #[cfg(windows)]
    {
        return probe_windows_npm(root, bin_dir).await;
    }
    #[cfg(not(windows))]
    {
        probe_version_output(&root.join(npm_relative_path()), bin_dir)
            .await
            .map(|_| ())
    }
}

#[cfg(windows)]
async fn probe_windows_npm(root: &Path, bin_dir: Option<&Path>) -> Result<(), AppCommandError> {
    let node = root.join(node_relative_path());
    let cli = root
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js");
    if !cli.is_file() {
        return Err(AppCommandError::invalid_input(
            "Managed Node.js payload is missing npm CLI",
        ));
    }
    run_probe(&node, &[cli.as_os_str(), OsStr::new("--version")], bin_dir)
        .await
        .map(|_| ())
}

async fn probe_version_output(
    command: &Path,
    bin_dir: Option<&Path>,
) -> Result<String, AppCommandError> {
    let output = run_probe(command, &[OsStr::new("--version")], bin_dir).await?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn run_probe(
    command: &Path,
    args: &[&OsStr],
    bin_dir: Option<&Path>,
) -> Result<Output, AppCommandError> {
    let mut process = crate::process::tokio_command(command);
    process.args(args);
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
    let output = tokio::time::timeout(PROBE_TIMEOUT, process.output())
        .await
        .map_err(|_| {
            AppCommandError::task_execution_failed("Managed tool probe timed out").with_detail(
                command
                    .file_name()
                    .unwrap_or_else(|| OsStr::new("unknown"))
                    .to_string_lossy()
                    .into_owned(),
            )
        })?
        .map_err(|error| {
            AppCommandError::task_execution_failed("Managed tool probe failed").with_detail(
                format!(
                    "command={}; error={error}",
                    command
                        .file_name()
                        .unwrap_or_else(|| OsStr::new("unknown"))
                        .to_string_lossy()
                ),
            )
        })?;
    ensure_probe_success(command, &output)?;
    Ok(output)
}

fn ensure_probe_success(command: &Path, output: &Output) -> Result<(), AppCommandError> {
    if output.status.success() {
        return Ok(());
    }
    Err(
        AppCommandError::invalid_input("Managed tool probe returned an unexpected version")
            .with_detail(format!(
                "command={}; exit_code={:?}; stdout={}; stderr={}",
                command
                    .file_name()
                    .unwrap_or_else(|| OsStr::new("unknown"))
                    .to_string_lossy(),
                output.status.code(),
                summarize_probe_output(&output.stdout),
                summarize_probe_output(&output.stderr),
            )),
    )
}

fn unexpected_version(tool_id: &str, expected: &str, actual: &str) -> AppCommandError {
    AppCommandError::invalid_input("Managed tool probe returned an unexpected version").with_detail(
        format!(
            "tool={tool_id}; expected={expected}; output={}",
            summarize_probe_output(actual.as_bytes())
        ),
    )
}

fn summarize_probe_output(bytes: &[u8]) -> String {
    let value = String::from_utf8_lossy(bytes).trim().replace('\n', " ");
    value.chars().take(512).collect()
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
        "browser-engine" => root.join(browser_engine_relative_path()).is_file(),
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

fn browser_engine_relative_path() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("chrome.exe")
    } else if cfg!(target_os = "macos") {
        Path::new("Google Chrome for Testing.app").join("Contents/MacOS/Google Chrome for Testing")
    } else {
        PathBuf::from("chrome")
    }
}

fn ensure_tool_layout(root: &Path, tool_id: &str) -> Result<(), AppCommandError> {
    has_tool_layout(root, tool_id)
        .then_some(())
        .ok_or_else(|| AppCommandError::invalid_input("Managed tool files are incomplete"))
}
