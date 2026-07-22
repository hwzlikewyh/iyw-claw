use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, ChildStderr, ChildStdout};

use super::companion_manifest::bounded_detail;
pub use super::companion_manifest::parse_companion_manifest;
use crate::user_memory::{CompanionHealthReason, CompanionHealthSnapshot, CompanionHealthStatus};

const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const REAP_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_STDOUT_BYTES: usize = 64 * 1024;
const MAX_STDERR_BYTES: usize = 8 * 1024;

type BoundedOutput = (Vec<u8>, bool);
type ProbeOutput = (ExitStatus, BoundedOutput, BoundedOutput);

pub async fn locate_healthy_companion() -> CompanionHealthSnapshot {
    let candidates = match tokio::task::spawn_blocking(discover_candidates).await {
        Ok(candidates) => candidates,
        Err(error) => {
            let mut health = base_snapshot(None);
            health.status = CompanionHealthStatus::ProbeFailed;
            health.reason = CompanionHealthReason::JoinFailed;
            health.detail = Some(bounded_detail(error.to_string()));
            return health;
        }
    };
    let mut failure = CompanionHealthSnapshot::default();
    for candidate in candidates {
        let health = probe_companion_path(candidate, DEFAULT_PROBE_TIMEOUT).await;
        if health.status == CompanionHealthStatus::Ready {
            return health;
        }
        if should_replace_failure(&failure, &health) {
            failure = health;
        }
    }
    failure
}

pub async fn probe_companion_path(path: PathBuf, timeout: Duration) -> CompanionHealthSnapshot {
    if let Err(health) = inspect_candidate(path.clone()).await {
        return health;
    }
    let mut command = crate::process::tokio_command(&path);
    command
        .arg("--capabilities")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return probe_failure(path, CompanionHealthReason::SpawnFailed, error),
    };
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return probe_failure(
            path,
            CompanionHealthReason::SpawnFailed,
            "pipes unavailable",
        );
    };
    match tokio::time::timeout(timeout, collect_output(&mut child, stdout, stderr)).await {
        Ok(Ok(output)) => health_from_output(path, output),
        Ok(Err(error)) => probe_failure(path, CompanionHealthReason::ExitFailed, error),
        Err(_) => {
            terminate_child(&mut child).await;
            let mut health = base_snapshot(Some(path));
            health.status = CompanionHealthStatus::Timeout;
            health.reason = CompanionHealthReason::ProbeTimeout;
            health.detail = Some(format!("capability probe exceeded {timeout:?}"));
            health
        }
    }
}

fn discover_candidates() -> Vec<PathBuf> {
    let filename = if cfg!(windows) {
        "iyw-claw-mcp.exe"
    } else {
        "iyw-claw-mcp"
    };
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("IYW_CLAW_MCP_BIN").filter(|value| !value.is_empty()) {
        candidates.push(absolute_candidate(PathBuf::from(path)));
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    {
        candidates.push(parent.join(filename));
    }
    if let Ok(path) = which::which(filename) {
        candidates.push(absolute_candidate(path));
    }
    deduplicate(candidates)
}

async fn inspect_candidate(path: PathBuf) -> Result<(), CompanionHealthSnapshot> {
    let inspected = tokio::task::spawn_blocking({
        let path = path.clone();
        move || inspect_candidate_sync(&path)
    })
    .await;
    match inspected {
        Ok(Ok(())) => Ok(()),
        Ok(Err((reason, detail))) => {
            let mut health = base_snapshot(Some(path));
            if reason != CompanionHealthReason::BinaryMissing {
                health.status = CompanionHealthStatus::ProbeFailed;
            }
            health.reason = reason;
            health.detail = Some(bounded_detail(detail));
            Err(health)
        }
        Err(error) => {
            let mut health = base_snapshot(Some(path));
            health.status = CompanionHealthStatus::ProbeFailed;
            health.reason = CompanionHealthReason::JoinFailed;
            health.detail = Some(bounded_detail(error.to_string()));
            Err(health)
        }
    }
}

fn inspect_candidate_sync(path: &Path) -> Result<(), (CompanionHealthReason, String)> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        let reason = if error.kind() == std::io::ErrorKind::NotFound {
            CompanionHealthReason::BinaryMissing
        } else {
            CompanionHealthReason::NotExecutable
        };
        (reason, error.to_string())
    })?;
    if !metadata.is_file() {
        return Err((
            CompanionHealthReason::NotExecutable,
            "not a regular file".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err((
                CompanionHealthReason::NotExecutable,
                "executable bit is not set".into(),
            ));
        }
    }
    Ok(())
}

async fn collect_output(
    child: &mut Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
) -> std::io::Result<ProbeOutput> {
    let (status, stdout, stderr) = tokio::join!(
        child.wait(),
        read_bounded(stdout, MAX_STDOUT_BYTES),
        read_bounded(stderr, MAX_STDERR_BYTES),
    );
    Ok((status?, stdout?, stderr?))
}

async fn read_bounded<R>(reader: R, limit: usize) -> std::io::Result<BoundedOutput>
where
    R: AsyncRead + Unpin,
{
    let mut reader = reader;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    let mut overflowed = false;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let retained = limit.saturating_sub(bytes.len()).min(read);
        bytes.extend_from_slice(&buffer[..retained]);
        overflowed |= retained < read;
    }
    Ok((bytes, overflowed))
}

fn health_from_output(path: PathBuf, output: ProbeOutput) -> CompanionHealthSnapshot {
    let (status, stdout, stderr) = output;
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr.0);
        return probe_failure(
            path,
            CompanionHealthReason::ExitFailed,
            format!("capability probe exited with {status}: {detail}"),
        );
    }
    if stdout.1 {
        return probe_failure(
            path,
            CompanionHealthReason::ManifestMalformed,
            "capability output exceeded the size limit",
        );
    }
    match String::from_utf8(stdout.0) {
        Ok(raw) => parse_companion_manifest(path, &raw),
        Err(error) => probe_failure(path, CompanionHealthReason::ManifestMalformed, error),
    }
}

async fn terminate_child(child: &mut Child) {
    let _ = child.start_kill();
    let _ = tokio::time::timeout(REAP_TIMEOUT, child.wait()).await;
}

fn probe_failure(
    path: PathBuf,
    reason: CompanionHealthReason,
    detail: impl ToString,
) -> CompanionHealthSnapshot {
    let mut health = base_snapshot(Some(path));
    health.status = CompanionHealthStatus::ProbeFailed;
    health.reason = reason;
    health.detail = Some(bounded_detail(detail.to_string()));
    health
}

fn base_snapshot(selected_path: Option<PathBuf>) -> CompanionHealthSnapshot {
    CompanionHealthSnapshot {
        selected_path,
        ..CompanionHealthSnapshot::default()
    }
}

fn absolute_candidate(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(&path))
            .unwrap_or(path)
    }
}

fn deduplicate(candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn should_replace_failure(
    current: &CompanionHealthSnapshot,
    next: &CompanionHealthSnapshot,
) -> bool {
    current.selected_path.is_none()
        || (current.status == CompanionHealthStatus::Missing
            && next.status != CompanionHealthStatus::Missing)
}
