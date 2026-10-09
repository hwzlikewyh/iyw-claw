// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl DriverProc {
    #[cfg(not(target_os = "macos"))]
    #[allow(clippy::type_complexity)]
    pub(in crate::computer::helper::driver_proc) async fn spawn(
        path: &Path,
        env: &[(String, String)],
        run_dir: &Path,
    ) -> Result<
        (
            ChildProc,
            Box<dyn AsyncRead + Send + Unpin>,
            Box<dyn tokio::io::AsyncWrite + Send + Unpin>,
            Box<dyn AsyncRead + Send + Unpin>,
        ),
        HelperError,
    > {
        let mut command = tokio::process::Command::new(path);
        command
            .args(["mcp", "--direct", "--no-overlay"])
            .env_clear()
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .current_dir(run_dir)
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
            .map_err(|e| unavailable(format!("could not start the driver: {e}")))?;
        let stdin = child.stdin.take().ok_or_else(|| unavailable("no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| unavailable("no stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| unavailable("no stderr"))?;
        Ok((
            ChildProc::Tokio {
                child: tokio::sync::Mutex::new(child),
                exited: std::sync::atomic::AtomicBool::new(false),
            },
            Box::new(stdout),
            Box::new(stdin),
            Box::new(stderr),
        ))
    }

    /// Whether the driver is still there to answer.
    pub fn alive(&self) -> bool {
        if self.client.is_closed() {
            return false;
        }
        match &self.child {
            #[cfg(target_os = "macos")]
            ChildProc::Mac(child) => !child.has_exited(),
            #[cfg(not(target_os = "macos"))]
            ChildProc::Tokio { exited, .. } => !exited.load(std::sync::atomic::Ordering::Acquire),
        }
    }

    pub async fn call(
        &self,
        tool: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<ToolCallResult, HelperError> {
        let result = self
            .client
            .call_tool(tool, arguments, timeout)
            .await
            .map_err(|e| match e {
                McpError::Closed => unavailable("the driver exited"),
                McpError::Timeout => {
                    HelperError::failed(format!("the driver did not answer {tool} in time"))
                }
                other => HelperError::failed(other.to_string()),
            })?;
        if result.is_error && result.code() == Some("session_ended") {
            // A driver whose session has ended refuses everything from then
            // on. It should not happen (the session is set never to idle
            // out); if it does, this driver is done and the next call starts
            // another.
            self.client.close();
            return Err(unavailable(
                "the driver ended its session; it is started again on the next call",
            ));
        }
        Ok(result)
    }

    /// Kill the driver at once — no time to finish what it is doing — and
    /// remove its home directory. Every call waiting on it fails now rather
    /// than when the pipe closes.
    pub async fn kill(&self) {
        self.client.close();
        match &self.child {
            #[cfg(target_os = "macos")]
            ChildProc::Mac(child) => {
                child.kill();
                let _ = child.wait().await;
            }
            #[cfg(not(target_os = "macos"))]
            ChildProc::Tokio { child, exited } => {
                let mut child = child.lock().await;
                let _ = child.start_kill();
                let _ = child.wait().await;
                exited.store(true, std::sync::atomic::Ordering::Release);
            }
        }
        let _ = std::fs::remove_dir_all(&self.run_dir);
    }

    /// Stop the driver and remove its home directory.
    pub async fn shutdown(&self) {
        match &self.child {
            #[cfg(target_os = "macos")]
            ChildProc::Mac(child) => {
                child.terminate();
                if tokio::time::timeout(Duration::from_secs(2), child.wait())
                    .await
                    .is_err()
                {
                    child.kill();
                    let _ = child.wait().await;
                }
            }
            #[cfg(not(target_os = "macos"))]
            ChildProc::Tokio { child, exited } => {
                let mut child = child.lock().await;
                let _ = child.start_kill();
                let _ = child.wait().await;
                exited.store(true, std::sync::atomic::Ordering::Release);
            }
        }
        let _ = std::fs::remove_dir_all(&self.run_dir);
    }
}
