use tokio::process::{Child, Command};

#[cfg(windows)]
#[path = "connection_job.rs"]
mod job;

/// Guard is owned by the protocol connection, not by an individual prompt.
pub(super) struct ConnectionChild {
    child: Option<Child>,
    #[cfg(windows)]
    job: Option<job::ConnectionJob>,
}

impl ConnectionChild {
    pub(super) fn spawn(command: &mut Command) -> std::io::Result<Self> {
        command.kill_on_drop(true);
        #[cfg(windows)]
        {
            let (child, job) = job::ConnectionJob::spawn(command)?;
            Ok(Self {
                child: Some(child),
                job,
            })
        }
        #[cfg(not(windows))]
        Ok(Self {
            child: Some(command.spawn()?),
        })
    }

    pub(super) fn child(&mut self) -> &mut Child {
        self.child.as_mut().expect("connection child remains owned")
    }

    pub(super) fn into_child(mut self) -> Child {
        #[cfg(windows)]
        self.job.take();
        self.child.take().expect("connection child remains owned")
    }

    pub(super) async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.child().wait().await
    }
}

impl Drop for ConnectionChild {
    fn drop(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        #[cfg(windows)]
        if let Some(job) = &self.job {
            // 仅在整个协议连接退出/取消时回收，不会在普通 turn 结束时触发。
            if job.terminate().is_ok() {
                return;
            }
        }
        if let Some(pid) = child.id() {
            let _ = kill_tree::blocking::kill_tree(pid);
        }
        let _ = child.start_kill();
    }
}
