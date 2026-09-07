use std::io;
use tokio::process::{Child, Command};

#[cfg(windows)]
#[path = "terminal_job.rs"]
mod job;

pub(super) struct TerminalProcess {
    pub(super) child: Child,
    #[cfg(windows)]
    job: Option<job::TerminalJob>,
}

impl TerminalProcess {
    pub(super) async fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(windows)]
        {
            let (child, job) = job::TerminalJob::spawn(command)?;
            Ok(Self { child, job })
        }
        #[cfg(not(windows))]
        {
            command.kill_on_drop(true);
            let child = crate::process::spawn_retrying_exec_busy(|| command.spawn()).await?;
            Ok(Self { child })
        }
    }

    pub(super) async fn terminate(&mut self) -> io::Result<()> {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            return job.terminate();
        }
        if let Some(pid) = self.child.id() {
            if let Err(error) = kill_tree::tokio::kill_tree(pid).await {
                tracing::warn!(pid, %error, "[ACP] terminal tree stop failed; stopping owned child");
                self.child.start_kill()?;
            }
        }
        Ok(())
    }

    pub(super) fn has_descendants(&self) -> io::Result<bool> {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            return job.has_processes();
        }
        Ok(false)
    }
}

impl Drop for TerminalProcess {
    fn drop(&mut self) {
        // 正常监视器只在输出结束且 Job 无活动成员后退出；其余 Drop 是所属
        // terminal 创建失败或任务被取消，必须回收这个明确拥有的进程树。
        #[cfg(windows)]
        if let Some(job) = &self.job {
            if let Err(error) = job.terminate() {
                tracing::warn!(%error, "[ACP] terminal scope cleanup failed");
            }
        }
    }
}
