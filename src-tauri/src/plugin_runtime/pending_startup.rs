pub(super) struct PendingStartup {
    pid: Option<u32>,
    stderr: tokio::task::AbortHandle,
    armed: bool,
}

impl PendingStartup {
    pub(super) fn new(child: &tokio::process::Child, stderr: &tokio::task::JoinHandle<()>) -> Self {
        Self {
            pid: child.id(),
            stderr: stderr.abort_handle(),
            armed: true,
        }
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PendingStartup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.stderr.abort();
        if let Some(pid) = self.pid {
            let config = kill_tree::Config {
                signal: "SIGKILL".into(),
                ..Default::default()
            };
            if kill_tree::blocking::kill_tree_with_config(pid, &config).is_err() {
                tracing::warn!(
                    pid,
                    "[plugin-runtime] cancelled startup process tree cleanup failed"
                );
            }
        }
    }
}
