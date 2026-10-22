use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::{library, upload_models::*};
use crate::acp::capability_policy::CapabilityRevocationMonitor;
use crate::app_error::{AppCommandError, AppErrorCode};

const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const WATCH_INTERVAL: Duration = Duration::from_secs(5);

pub struct UploadJob {
    pub input: BeginUpload,
    pub path: tempfile::TempPath,
    pub cancel: CancellationToken,
    pub monitor: CapabilityRevocationMonitor,
    pub sent: AtomicU64,
    pub state: Mutex<UploadState>,
}

pub struct UploadState {
    pub phase: UploadPhase,
    pub received: u64,
    pub object: Option<UploadedObject>,
    pub registration_attempted: bool,
    pub error: Option<String>,
    pub touched: Instant,
}

impl UploadJob {
    pub async fn snapshot(&self) -> UploadStatus {
        let state = self.state.lock().await;
        UploadStatus {
            upload_id: self.input.upload_id.clone(),
            phase: state.phase,
            received_bytes: state.received,
            sent_bytes: self.sent.load(Ordering::Relaxed),
            total_bytes: self.input.expected_bytes,
            error: state.error.clone(),
        }
    }

    pub async fn transition(&self, phase: UploadPhase) {
        let mut state = self.state.lock().await;
        state.phase = phase;
        state.error = None;
        state.touched = Instant::now();
    }
}

pub fn store() -> &'static Mutex<HashMap<String, Arc<UploadJob>>> {
    static STORE: OnceLock<Mutex<HashMap<String, Arc<UploadJob>>>> = OnceLock::new();
    STORE.get_or_init(Mutex::default)
}

pub async fn lookup(
    conn: &DatabaseConnection,
    input: &UploadRef,
) -> Result<Arc<UploadJob>, AppCommandError> {
    library::require_scope(conn, &input.scope).await?;
    let job = store()
        .lock()
        .await
        .get(&input.upload_id)
        .cloned()
        .filter(|job| job.input.scope == input.scope)
        .ok_or_else(|| AppCommandError::not_found("上传已取消或过期，请重新选择文件"))?;
    Ok(job)
}

pub async fn insert(job: Arc<UploadJob>) -> Result<(), AppCommandError> {
    let mut uploads = store().lock().await;
    let reserved: u64 = uploads.values().map(|job| job.input.expected_bytes).sum();
    if uploads.contains_key(&job.input.upload_id) {
        return Err(AppCommandError::new(
            AppErrorCode::Conflict,
            "上传标识已存在，请重试",
        ));
    }
    if uploads.len() >= MAX_UPLOADS
        || reserved.saturating_add(job.input.expected_bytes) > MAX_STAGED_BYTES
    {
        return Err(AppCommandError::task_execution_failed(
            "上传任务已满，请等待当前上传完成",
        ));
    }
    uploads.insert(job.input.upload_id.clone(), job);
    Ok(())
}

pub fn watch(conn: DatabaseConnection, job: Arc<UploadJob>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(WATCH_INTERVAL);
        loop {
            tokio::select! {
                _ = job.cancel.cancelled() => break,
                _ = interval.tick() => {
                    let idle = job.state.lock().await.touched.elapsed() > IDLE_TIMEOUT;
                    let revoked = library::require_scope(&conn, &job.input.scope).await.is_err();
                    if idle || revoked || job.monitor.error_if_revoked().is_err() { break; }
                }
            }
        }
        job.cancel.cancel();
        let mut uploads = store().lock().await;
        if uploads
            .get(&job.input.upload_id)
            .is_some_and(|current| Arc::ptr_eq(current, &job))
        {
            uploads.remove(&job.input.upload_id);
        }
        tracing::debug!(upload_id = %job.input.upload_id, "[knowledge] upload lease released");
    });
}

pub fn io_error(error: std::io::Error) -> AppCommandError {
    tracing::warn!(kind = ?error.kind(), "[knowledge] upload staging I/O failed");
    AppCommandError::io_error("上传暂存失败，请检查磁盘空间后重试")
        .with_detail(format!("kind={:?}", error.kind()))
}
