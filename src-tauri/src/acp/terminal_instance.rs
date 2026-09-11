use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sacp::schema::TerminalExitStatus;
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::process::TerminalProcess;
use super::{enforce_output_limit, map_exit_status, TerminalRuntimeError};

const STOP_WAIT: Duration = Duration::from_secs(5);
const CANCELLED_OUTPUT_DRAIN: Duration = Duration::from_secs(2);
const DESCENDANT_POLL: Duration = Duration::from_secs(1);

#[derive(Default, Clone)]
pub(super) struct TerminalSnapshot {
    pub(super) output: String,
    pub(super) output_base_offset: u64,
    pub(super) truncated: bool,
    pub(super) exit_status: Option<TerminalExitStatus>,
    error: Option<String>,
}

pub(super) struct TerminalInstance {
    pub(super) session_id: String,
    output_limit: usize,
    pub(super) snapshot: Mutex<TerminalSnapshot>,
    active_count: Arc<AtomicUsize>,
    active: AtomicBool,
    changed: Notify,
    stop_requested: Notify,
    stop: CancellationToken,
}

// Guard 跨越 create 的注册 await 和整个 monitor；取消/恐慌时也不能
// 留下持有 TerminalInstance、管道和输出缓冲区的脱管 reader。
#[derive(Default)]
pub(super) struct TerminalReaders(pub(super) Vec<JoinHandle<()>>);

impl Drop for TerminalReaders {
    fn drop(&mut self) {
        for reader in &self.0 {
            reader.abort();
        }
    }
}

impl TerminalInstance {
    pub(super) fn new(
        session_id: String,
        output_limit: u64,
        active_count: Arc<AtomicUsize>,
    ) -> Self {
        active_count.fetch_add(1, Ordering::AcqRel);
        Self {
            session_id,
            output_limit: usize::try_from(output_limit).unwrap_or(usize::MAX),
            snapshot: Mutex::new(TerminalSnapshot::default()),
            active_count,
            active: AtomicBool::new(true),
            changed: Notify::new(),
            stop_requested: Notify::new(),
            stop: CancellationToken::new(),
        }
    }

    pub(super) fn request_stop(&self) {
        self.stop.cancel();
        self.stop_requested.notify_one();
    }

    pub(super) fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    fn mark_exited(&self) {
        if self.active.swap(false, Ordering::AcqRel) {
            self.active_count.fetch_sub(1, Ordering::AcqRel);
        }
        self.changed.notify_waiters();
    }

    pub(super) async fn append_output(&self, text: &str) {
        let mut snapshot = self.snapshot.lock().await;
        snapshot.output.push_str(text);
        let removed = enforce_output_limit(&mut snapshot.output, self.output_limit);
        if removed > 0 {
            snapshot.truncated = true;
            snapshot.output_base_offset =
                snapshot.output_base_offset.saturating_add(removed as u64);
        }
    }

    pub(super) async fn record_read_error(&self, error: std::io::Error) {
        tracing::warn!(%error, "[ACP] terminal output reader failed");
        self.snapshot.lock().await.error = Some(format!("terminal output read failed: {error}"));
        self.changed.notify_waiters();
    }

    pub(super) async fn refresh_exit_status(&self) -> Result<(), TerminalRuntimeError> {
        match self.snapshot.lock().await.error.clone() {
            Some(error) => Err(TerminalRuntimeError::Internal(error)),
            None => Ok(()),
        }
    }

    pub(super) async fn wait_for_exit(&self) -> Result<TerminalExitStatus, TerminalRuntimeError> {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            self.refresh_exit_status().await?;
            if let Some(status) = self.snapshot.lock().await.exit_status.clone() {
                return Ok(status);
            }
            changed.await;
        }
    }

    pub(super) async fn kill_command(&self) -> Result<(), TerminalRuntimeError> {
        self.request_stop();
        let stopped = async {
            loop {
                let changed = self.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if !self.is_active() {
                    return;
                }
                changed.await;
            }
        };
        tokio::time::timeout(STOP_WAIT, stopped)
            .await
            .map_err(|_| {
                TerminalRuntimeError::Internal("terminal stop timed out; ownership retained".into())
            })?;
        Ok(())
    }

    pub(super) async fn snapshot(&self) -> TerminalSnapshot {
        self.snapshot.lock().await.clone()
    }

    pub(super) async fn monitor(
        self: Arc<Self>,
        mut process: TerminalProcess,
        mut readers: TerminalReaders,
    ) {
        // 唯一的 Child 所有者等待 OS 退出事件，output/kill 不再争用 Child 锁。
        let status = self.wait_for_process(&mut process).await;
        self.drain_readers(&mut process, &mut readers.0).await;
        // Job 内仍有后代计算时不发布“已结束”，防止调用方收到退出状态后
        // 自动 release 把仍在计算但没有输出的后台任务终止。
        self.wait_for_descendants(&mut process).await;
        match status {
            Ok(status) => self.snapshot.lock().await.exit_status = Some(map_exit_status(status)),
            Err(error) => {
                tracing::error!(%error, "[ACP] terminal exit monitor failed");
                self.snapshot.lock().await.error = Some(format!("terminal exit failed: {error}"));
            }
        }
        self.changed.notify_waiters();
        self.mark_exited();
    }

    async fn stop_process(&self, process: &mut TerminalProcess) {
        if let Err(error) = process.terminate().await {
            tracing::error!(%error, "[ACP] owned terminal tree stop failed");
        }
    }

    async fn wait_for_process(
        &self,
        process: &mut TerminalProcess,
    ) -> std::io::Result<std::process::ExitStatus> {
        loop {
            tokio::select! {
                status = process.child.wait() => return status,
                _ = self.stop_requested.notified() => self.stop_process(process).await,
            }
        }
    }

    async fn drain_readers(&self, process: &mut TerminalProcess, readers: &mut [JoinHandle<()>]) {
        let abort_handles: Vec<_> = readers.iter().map(JoinHandle::abort_handle).collect();
        let drained = futures::future::join_all(readers.iter_mut());
        tokio::pin!(drained);
        // 正常结束必须等 EOF，不能把安静或父进程退出当作输出已经结束。
        tokio::select! {
            _ = &mut drained => return,
            _ = self.stop.cancelled() => self.stop_process(process).await,
        }
        // 只有明确取消后才限制 drain 等待；超时仍由当前任务回收自己的 reader。
        if tokio::time::timeout(CANCELLED_OUTPUT_DRAIN, &mut drained)
            .await
            .is_err()
        {
            for handle in abort_handles {
                handle.abort();
            }
            let _ = drained.await;
        }
    }

    async fn wait_for_descendants(&self, process: &mut TerminalProcess) {
        let mut stopped = false;
        let mut query_failed = false;
        loop {
            match process.has_descendants() {
                Ok(false) => return,
                Ok(true) => {}
                Err(error) => {
                    if !query_failed {
                        tracing::warn!(%error, "[ACP] terminal tree state unknown; retaining ownership");
                        query_failed = true;
                    }
                }
            }
            tokio::select! {
                _ = self.stop.cancelled(), if !stopped => {
                    self.stop_process(process).await;
                    stopped = true;
                }
                _ = self.stop_requested.notified() => self.stop_process(process).await,
                _ = tokio::time::sleep(DESCENDANT_POLL) => {}
            }
        }
    }
}

impl Drop for TerminalInstance {
    fn drop(&mut self) {
        self.mark_exited();
    }
}
