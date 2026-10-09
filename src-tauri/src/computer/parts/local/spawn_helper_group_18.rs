// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(not(target_os = "macos"))]
pub(super) fn spawn_helper(
    path: &std::path::Path,
) -> Result<(HelperChild, Io, PeerFd), BackendError> {
    let mut command = tokio::process::Command::new(path);
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| BackendError::Unavailable(format!("could not start the helper: {e}")))?;
    let missing = || BackendError::Unavailable("the helper has no stdio".into());
    let stdin = child.stdin.take().ok_or_else(missing)?;
    let stdout = child.stdout.take().ok_or_else(missing)?;
    let stderr = child.stderr.take().ok_or_else(missing)?;
    Ok((
        HelperChild::Tokio(Mutex::new(child)),
        (Box::new(stdout), Box::new(stdin), Box::new(stderr)),
        None,
    ))
}
