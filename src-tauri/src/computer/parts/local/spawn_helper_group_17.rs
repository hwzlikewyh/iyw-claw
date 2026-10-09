// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(target_os = "macos")]
pub(super) fn spawn_helper(
    path: &std::path::Path,
) -> Result<(HelperChild, Io, PeerFd), BackendError> {
    use crate::computer::spawn::{spawn, ChildFd, SpawnSpec};
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;

    let unavailable =
        |what: &str, e: std::io::Error| BackendError::Unavailable(format!("{what}: {e}"));
    let (ours, theirs) = UnixStream::pair().map_err(|e| unavailable("socketpair", e))?;
    let (err_ours, err_theirs) = UnixStream::pair().map_err(|e| unavailable("socketpair", e))?;
    let env = helper_environment();
    let requirement = helper_launch_requirement();
    let child = spawn(&SpawnSpec {
        program: path,
        args: &[],
        env: &env,
        stdio: [
            ChildFd::Inherit(theirs.as_raw_fd()),
            ChildFd::Inherit(theirs.as_raw_fd()),
            ChildFd::Inherit(err_theirs.as_raw_fd()),
        ],
        disclaim: true,
        suspended: false,
        launch_requirement: requirement.as_deref(),
    })
    .map_err(|e| unavailable("could not start the helper", e))?;
    drop(theirs);
    drop(err_theirs);
    let peer_fd = ours.as_raw_fd();
    let to_tokio = |s: UnixStream| -> Result<tokio::net::UnixStream, BackendError> {
        s.set_nonblocking(true)
            .and_then(|_| tokio::net::UnixStream::from_std(s))
            .map_err(|e| unavailable("socket", e))
    };
    let (reader, writer) = to_tokio(ours)?.into_split();
    let (err_reader, _) = to_tokio(err_ours)?.into_split();
    // `peer_fd` stays valid for as long as the split halves live: the read
    // half is owned by the reader task, the only place it is read after
    // `launch` returns.
    Ok((
        HelperChild::Mac(child),
        (Box::new(reader), Box::new(writer), Box::new(err_reader)),
        Some(peer_fd),
    ))
}

/// The helper's whole environment. It reads nothing from it that decides
/// anything; this is here so the few system calls that look are not
/// surprised.
#[cfg(target_os = "macos")]
pub(super) fn helper_environment() -> Vec<(String, String)> {
    vec![(
        "PATH".to_string(),
        "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    )]
}

/// What the kernel holds a helper launch to in a release build: this team's
/// Developer ID build of the helper, and nothing else from that path.
#[cfg(target_os = "macos")]
pub(super) fn helper_launch_requirement() -> Option<Vec<u8>> {
    HELPER_TEAM_ID
        .filter(|team| !team.trim().is_empty())
        .map(|team| crate::computer::launch_req::signed_by(team, HELPER_SIGNING_ID))
}

/// Have a helper started for the purpose ask macOS for `permission`, and say
/// whether the system put up its own dialog. Started exactly as the serving
/// helper is — its own TCC principal, under the same launch requirement — so
/// the request names the helper and lists it in System Settings; it serves
/// no one and exits once it has answered.
#[cfg(target_os = "macos")]
pub(super) async fn ask_for_permission(
    path: &std::path::Path,
    permission: OsPermission,
) -> Result<PermissionAsked, BackendError> {
    use crate::computer::protocol::REQUEST_PERMISSION_ARG;
    use crate::computer::spawn::{spawn, ChildFd, SpawnSpec};
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;
    use tokio::io::AsyncReadExt;

    let unavailable =
        |what: &str, e: std::io::Error| BackendError::Unavailable(format!("{what}: {e}"));
    let (ours, theirs) = UnixStream::pair().map_err(|e| unavailable("socketpair", e))?;
    let env = helper_environment();
    let requirement = helper_launch_requirement();
    let child = spawn(&SpawnSpec {
        program: path,
        args: &[REQUEST_PERMISSION_ARG, permission.arg()],
        env: &env,
        stdio: [
            ChildFd::Null,
            ChildFd::Inherit(theirs.as_raw_fd()),
            ChildFd::Null,
        ],
        disclaim: true,
        suspended: false,
        launch_requirement: requirement.as_deref(),
    })
    .map_err(|e| unavailable("could not start the helper to ask", e))?;
    drop(theirs);
    let read = async {
        ours.set_nonblocking(true)?;
        let stream = tokio::net::UnixStream::from_std(ours)?;
        let mut out = Vec::new();
        stream
            .take(MAX_ASK_OUTPUT + 1)
            .read_to_end(&mut out)
            .await?;
        std::io::Result::Ok(out)
    };
    let out = tokio::time::timeout(PERMISSION_ASK_TIMEOUT, read).await;
    // Done or not, it is not left behind.
    child.kill();
    let _ = child.wait().await;
    let out = match out {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(unavailable("could not read the helper's answer", e)),
        Err(_) => {
            return Err(BackendError::Unavailable(
                "the helper asking for the permission did not answer in time".into(),
            ))
        }
    };
    if out.len() as u64 > MAX_ASK_OUTPUT {
        return Err(BackendError::Failed(
            "the helper asking for the permission answered with more than one short line".into(),
        ));
    }
    serde_json::from_slice(out.trim_ascii()).map_err(|e| {
        BackendError::Failed(format!(
            "the helper asking for the permission answered oddly: {e}"
        ))
    })
}
