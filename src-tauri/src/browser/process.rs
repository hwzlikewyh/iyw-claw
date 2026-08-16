use std::path::Path;
use std::time::{Duration, Instant};

use sysinfo::{Pid, System};
use tokio::process::Command;

use super::error::{BrowserError, BrowserErrorCode};

const KILL_EXIT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub(super) struct ProcessRecord {
    pub pid: u32,
    pub started_at: u64,
    pub label: String,
    pub executable: Option<std::path::PathBuf>,
}

pub(super) fn configure_hidden_process(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

pub(super) fn capture_process(pid: u32, label: impl Into<String>) -> Option<ProcessRecord> {
    let system = System::new_all();
    let process = system.process(Pid::from_u32(pid))?;
    Some(ProcessRecord {
        pid,
        started_at: process.start_time(),
        label: label.into(),
        executable: process.exe().map(Path::to_path_buf),
    })
}

pub(super) fn process_matches(record: &ProcessRecord) -> bool {
    let system = System::new_all();
    system
        .process(Pid::from_u32(record.pid))
        .is_some_and(|process| {
            process.start_time() == record.started_at
                && record.executable.as_ref().is_none_or(|expected| {
                    process
                        .exe()
                        .is_some_and(|actual| same_path(actual, expected))
                })
        })
}

pub(super) fn process_matches_executable(record: &ProcessRecord, executable: &Path) -> bool {
    let system = System::new_all();
    system
        .process(Pid::from_u32(record.pid))
        .filter(|process| process.start_time() == record.started_at)
        .and_then(|process| process.exe())
        .is_some_and(|actual| same_path(actual, executable))
}

pub(super) fn find_processes_by_executable_arg(
    executable: &Path,
    argument_fragment: &str,
    label: &str,
) -> Vec<ProcessRecord> {
    let system = System::new_all();
    system
        .processes()
        .values()
        .filter(|process| {
            process
                .exe()
                .is_some_and(|path| same_path(path, executable))
                && process
                    .cmd()
                    .iter()
                    .any(|argument| argument.contains(argument_fragment))
        })
        .map(|process| ProcessRecord {
            pid: process.pid().as_u32(),
            started_at: process.start_time(),
            label: label.to_string(),
            executable: Some(executable.to_path_buf()),
        })
        .collect()
}

pub(super) fn find_processes_by_exact_session(
    executable: &Path,
    session: &str,
    label: &str,
) -> Vec<ProcessRecord> {
    let system = System::new_all();
    system
        .processes()
        .values()
        .filter(|process| {
            process
                .exe()
                .is_some_and(|path| same_path(path, executable))
                && process
                    .cmd()
                    .windows(2)
                    .any(|arguments| arguments[0] == "--session" && arguments[1] == session)
        })
        .map(|process| ProcessRecord {
            pid: process.pid().as_u32(),
            started_at: process.start_time(),
            label: label.to_string(),
            executable: Some(executable.to_path_buf()),
        })
        .collect()
}

pub(super) async fn wait_for_exit(record: &ProcessRecord, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !process_matches(record) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    !process_matches(record)
}

pub(super) async fn kill_tree_checked(record: &ProcessRecord) -> Result<(), BrowserError> {
    if !process_matches(record) {
        return Ok(());
    }
    tracing::warn!(
        target: "iyw_claw_browser",
        pid = record.pid,
        process_label = %record.label,
        "browser process required kill-tree fallback"
    );
    kill_tree::tokio::kill_tree(record.pid).await.map_err(|_| {
        BrowserError::new(
            BrowserErrorCode::BrowserInternal,
            "A browser process could not be stopped",
        )
    })?;
    if wait_for_exit(record, KILL_EXIT_TIMEOUT).await {
        return Ok(());
    }
    Err(BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "A browser process remained alive after forced shutdown",
    ))
}

pub(super) async fn wait_for_pid_file(
    path: &Path,
    executable: &Path,
    timeout: Duration,
) -> Result<ProcessRecord, BrowserError> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(value) = tokio::fs::read_to_string(path).await {
            if let Ok(pid) = value.trim().parse::<u32>() {
                if let Some(record) = capture_process(pid, "agent-browser-daemon") {
                    if process_matches_executable(&record, executable) {
                        return Ok(record);
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(BrowserError::new(
        BrowserErrorCode::BrowserRuntimeStartTimeout,
        "The browser controller did not publish its process identity",
    )
    .retryable(true))
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    if cfg!(target_os = "windows") {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}
