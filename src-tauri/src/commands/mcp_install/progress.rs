use std::future::Future;

use crate::app_error::AppCommandError;

use super::Work;

tokio::task_local! { pub(super) static CURRENT: Work; }

pub(crate) fn stage(phase: &str) -> Result<(), AppCommandError> {
    CURRENT
        .try_with(|work| {
            let mut view = work.view.lock().map_err(|_| unavailable())?;
            if view.cancellable && work.cancel.is_cancelled() {
                return Err(cancelled());
            }
            view.phase = phase.into();
            if phase != "downloading" {
                view.received_bytes = 0;
                view.total_bytes = None;
            }
            Ok(())
        })
        .unwrap_or(Ok(()))
}

pub(crate) fn download(received: u64, total: Option<u64>) -> Result<(), AppCommandError> {
    CURRENT
        .try_with(|work| {
            if work.cancel.is_cancelled() {
                return Err(cancelled());
            }
            let mut view = work.view.lock().map_err(|_| unavailable())?;
            view.phase = "downloading".into();
            view.received_bytes = received;
            view.total_bytes = total;
            Ok(())
        })
        .unwrap_or(Ok(()))
}

// 提交目录与凭据时不丢弃 future；取消只发生在可安全重试的准备阶段。
pub(crate) fn begin_commit() -> Result<(), AppCommandError> {
    CURRENT
        .try_with(|work| {
            let mut view = work.view.lock().map_err(|_| unavailable())?;
            if work.cancel.is_cancelled() {
                return Err(cancelled());
            }
            view.phase = "committing".into();
            view.cancellable = false;
            Ok(())
        })
        .unwrap_or(Ok(()))
}

pub(crate) async fn cancellable<F, T>(future: F) -> Result<T, AppCommandError>
where
    F: Future<Output = Result<T, AppCommandError>>,
{
    let cancel = CURRENT.try_with(|work| work.cancel.clone()).ok();
    match cancel {
        Some(cancel) => tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(cancelled()),
            result = future => result,
        },
        None => future.await,
    }
}

pub(crate) fn cancelled() -> AppCommandError {
    AppCommandError::task_execution_failed("连接器安装已取消，原有配置保持不变")
}

fn unavailable() -> AppCommandError {
    AppCommandError::configuration_invalid("安装任务状态不可用")
}
