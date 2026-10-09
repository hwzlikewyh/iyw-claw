// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl DriverProc {
    /// Launch the driver at `path` for this platform's pin. Fails with
    /// [`HelperErrorCode::DriverRejected`] for any file or image that is not
    /// the pinned build, and [`HelperErrorCode::DriverUnavailable`] for one
    /// that is and would not start.
    ///
    /// `stopped` is asked once more just before the driver is spawned, after
    /// the file has been hashed — which takes as long as the disk makes it —
    /// so a Stop, or iyw-claw leaving, while that runs spawns nothing, and what
    /// is left of a start once a driver exists is bounded (see `SHUTDOWN_GRACE`
    /// in the helper).
    pub async fn launch(
        path: &Path,
        artifact: &DriverArtifact,
        stopped: impl Fn() -> Result<(), HelperError>,
    ) -> Result<Self, HelperError> {
        if !path.is_absolute() {
            return Err(rejected("the driver path is not absolute"));
        }
        let digest = file_sha256(path).map_err(|e| {
            unavailable(format!(
                "could not read the driver at {}: {e}",
                path.display()
            ))
        })?;
        if digest != artifact.executable_sha256 {
            return Err(rejected(format!(
                "{} is not the pinned cua-driver {} (sha256 {digest})",
                path.display(),
                driver::DRIVER_VERSION
            )));
        }

        let base =
            helper_data_dir().ok_or_else(|| unavailable("no home directory for this account"))?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let run_dir = base
            .join("runs")
            .join(format!("{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(run_dir.join("tmp"))
            .map_err(|e| unavailable(format!("could not create {}: {e}", run_dir.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&run_dir, std::fs::Permissions::from_mode(0o700));
        }
        if let Err(e) = write_driver_config(&run_dir) {
            let _ = std::fs::remove_dir_all(&run_dir);
            return Err(unavailable(format!(
                "could not write the driver's configuration: {e}"
            )));
        }
        let env = driver_environment(&run_dir);

        if let Err(halted) = stopped() {
            let _ = std::fs::remove_dir_all(&run_dir);
            return Err(halted);
        }
        let launched = Self::spawn(path, &env, &run_dir).await;
        let (child, reader, writer, stderr) = match launched {
            Ok(parts) => parts,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&run_dir);
                return Err(e);
            }
        };
        tokio::spawn(forward_stderr(stderr));
        let client = McpClient::start(reader, writer);
        let mut proc = Self {
            client,
            child,
            run_dir,
            full_size_captures: false,
        };
        if let Err(e) = proc.client.initialize(INITIALIZE_TIMEOUT).await {
            proc.shutdown().await;
            return Err(unavailable(format!("the driver did not start: {e}")));
        }
        proc.full_size_captures = proc.captures_at_full_size().await;
        if !proc.full_size_captures {
            tracing::warn!(
                "the driver did not take its full-size capture setting; pointing by \
                 coordinates is off for this driver"
            );
        }
        Ok(proc)
    }

    /// Whether the driver runs with the configuration written for it: no
    /// ceiling on a capture's size. Asked, not assumed — a driver that read
    /// its configuration from somewhere else would scale every capture and
    /// every click with it.
    pub(in crate::computer::helper::driver_proc) async fn captures_at_full_size(&self) -> bool {
        match self
            .client
            .call_tool("get_config", serde_json::json!({}), CONFIG_TIMEOUT)
            .await
        {
            Ok(result) if !result.is_error => {
                result
                    .structured
                    .as_ref()
                    .and_then(|s| s.get("max_image_dimension"))
                    .and_then(Value::as_u64)
                    == Some(0)
            }
            _ => false,
        }
    }

    /// See [`DriverProc::full_size_captures`].
    pub fn full_size_captures(&self) -> bool {
        self.full_size_captures
    }

    #[cfg(target_os = "macos")]
    #[allow(clippy::type_complexity)]
    pub(in crate::computer::helper::driver_proc) async fn spawn(
        path: &Path,
        env: &[(String, String)],
        _run_dir: &Path,
    ) -> Result<
        (
            ChildProc,
            Box<dyn AsyncRead + Send + Unpin>,
            Box<dyn tokio::io::AsyncWrite + Send + Unpin>,
            Box<dyn AsyncRead + Send + Unpin>,
        ),
        HelperError,
    > {
        use crate::computer::spawn::{spawn, ChildFd, SpawnSpec};
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;

        // One socket for stdin+stdout (the driver reads and writes MCP on it),
        // one for stderr. A socket is as good as a pipe to the driver, and a
        // single bidirectional one is one descriptor to hand over instead of
        // two.
        let (io_ours, io_theirs) =
            UnixStream::pair().map_err(|e| unavailable(format!("socketpair: {e}")))?;
        let (err_ours, err_theirs) =
            UnixStream::pair().map_err(|e| unavailable(format!("socketpair: {e}")))?;
        let requirement = driver_launch_requirement()?;
        let child = spawn(&SpawnSpec {
            program: path,
            args: &["mcp", "--direct", "--no-overlay"],
            env,
            stdio: [
                ChildFd::Inherit(io_theirs.as_raw_fd()),
                ChildFd::Inherit(io_theirs.as_raw_fd()),
                ChildFd::Inherit(err_theirs.as_raw_fd()),
            ],
            // The driver's TCC requests must be charged to the helper, which
            // is the whole reason it runs under the helper.
            disclaim: false,
            suspended: true,
            launch_requirement: Some(&requirement),
        })
        .map_err(|e| unavailable(format!("could not start the driver: {e}")))?;
        drop(io_theirs);
        drop(err_theirs);

        if let Err(why) = verify_running_driver(child.pid()) {
            child.kill();
            let _ = child.wait().await;
            return Err(rejected(why));
        }
        if let Err(e) = child.resume() {
            child.kill();
            return Err(unavailable(format!("could not resume the driver: {e}")));
        }

        let to_tokio = |s: UnixStream| -> Result<tokio::net::UnixStream, HelperError> {
            s.set_nonblocking(true)
                .and_then(|_| tokio::net::UnixStream::from_std(s))
                .map_err(|e| unavailable(format!("socket: {e}")))
        };
        let (reader, writer) = to_tokio(io_ours)?.into_split();
        let (err_reader, _) = to_tokio(err_ours)?.into_split();
        Ok((
            ChildProc::Mac(child),
            Box::new(reader),
            Box::new(writer),
            Box::new(err_reader),
        ))
    }
}
