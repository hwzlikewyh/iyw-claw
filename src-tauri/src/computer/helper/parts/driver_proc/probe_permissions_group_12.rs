// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Which of the helper's permissions are in force, as a fresh copy of the
/// pinned driver at `path` reports them.
///
/// A fresh process because macOS keeps a process's first "not granted" for
/// the rest of its life: the helper asking itself would go on hearing "no"
/// after the person has said yes in System Settings. (Raising a request is
/// not done here: the driver's own request asks for every missing
/// permission at once, and a person who pressed the button for one should
/// see the dialog for that one — iyw-claw starts a helper for each instead.)
/// The copy does not disclaim, so TCC answers it for its responsible
/// process, the helper; and
/// it is started as the driver is — under the launch requirement, suspended,
/// its running image checked, then resumed — since it runs with the helper's
/// grants too. (The file is not hashed first: the kernel refuses any other
/// image, and a damaged download is the next launch's to report.)
#[cfg(target_os = "macos")]
pub async fn probe_permissions(path: &Path) -> Result<PermissionReport, HelperError> {
    use crate::computer::spawn::{spawn, ChildFd, SpawnSpec};
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;
    use tokio::io::AsyncReadExt;

    #[derive(serde::Deserialize)]
    struct Probe {
        pub(in crate::computer::helper::driver_proc) accessibility: bool,
        pub(in crate::computer::helper::driver_proc) screen_recording: bool,
    }

    if !path.is_absolute() {
        return Err(rejected("the driver path is not absolute"));
    }
    // The driver's own environment, homed in the helper's directory: the
    // probe reads nothing from there, and gets nothing of the helper's.
    let home =
        helper_data_dir().ok_or_else(|| unavailable("no home directory for this account"))?;
    let env = driver_environment(&home);
    let (ours, theirs) = UnixStream::pair().map_err(|e| unavailable(format!("socketpair: {e}")))?;
    let requirement = driver_launch_requirement()?;
    let child = spawn(&SpawnSpec {
        program: path,
        args: &[PERMISSION_PROBE_ARG],
        env: &env,
        stdio: [
            ChildFd::Null,
            ChildFd::Inherit(theirs.as_raw_fd()),
            ChildFd::Null,
        ],
        disclaim: false,
        suspended: true,
        launch_requirement: Some(&requirement),
    })
    .map_err(|e| unavailable(format!("could not start the permission check: {e}")))?;
    drop(theirs);
    if let Err(why) = verify_running_driver(child.pid()) {
        child.kill();
        let _ = child.wait().await;
        return Err(rejected(why));
    }
    if let Err(e) = child.resume() {
        child.kill();
        let _ = child.wait().await;
        return Err(unavailable(format!(
            "could not resume the permission check: {e}"
        )));
    }
    let read = async {
        ours.set_nonblocking(true)?;
        let stream = tokio::net::UnixStream::from_std(ours)?;
        let mut out = Vec::new();
        stream
            .take(MAX_PROBE_OUTPUT + 1)
            .read_to_end(&mut out)
            .await?;
        std::io::Result::Ok(out)
    };
    let out = tokio::time::timeout(PROBE_TIMEOUT, read).await;
    // Done or not, it is not left behind.
    child.kill();
    let _ = child.wait().await;
    let out = match out {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            return Err(unavailable(format!(
                "could not read the permission check: {e}"
            )))
        }
        Err(_) => return Err(unavailable("the permission check did not answer in time")),
    };
    if out.len() as u64 > MAX_PROBE_OUTPUT {
        return Err(HelperError::failed(
            "the permission check answered with more than one short line",
        ));
    }
    let probe: Probe = serde_json::from_slice(out.trim_ascii())
        .map_err(|e| HelperError::failed(format!("the permission check answered oddly: {e}")))?;
    Ok(PermissionReport {
        required: true,
        accessibility: probe.accessibility,
        screen_recording: probe.screen_recording,
    })
}
