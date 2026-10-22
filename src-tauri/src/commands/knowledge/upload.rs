use std::sync::{atomic::AtomicU64, Arc};
use std::time::Instant;

use base64::Engine;
use sea_orm::DatabaseConnection;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::{
    library,
    upload_models::*,
    upload_store::{self, UploadJob, UploadState},
};
use crate::acp::capability_policy::monitor_file_upload;
use crate::app_error::{AppCommandError, AppErrorCode};

pub async fn begin(
    conn: &DatabaseConnection,
    input: BeginUpload,
) -> Result<UploadStatus, AppCommandError> {
    input.validate()?;
    let cancel = CancellationToken::new();
    let monitor = monitor_file_upload(Some(cancel.clone())).await?;
    let client = library::scoped_client(conn, &input.scope).await?;
    let folders = client.folders(input.category).await?;
    if !contains(&folders, &input.folder_id) {
        return Err(AppCommandError::permission_denied(
            "目标目录已失效或无权上传",
        ));
    }
    library::require_scope(conn, &input.scope).await?;
    let path = tempfile::Builder::new()
        .prefix("iyw-knowledge-")
        .tempfile()
        .map_err(upload_store::io_error)?
        .into_temp_path();
    let job = Arc::new(UploadJob {
        input,
        path,
        cancel,
        monitor,
        sent: AtomicU64::new(0),
        state: Mutex::new(UploadState {
            phase: UploadPhase::Receiving,
            received: 0,
            object: None,
            registration_attempted: false,
            error: None,
            touched: Instant::now(),
        }),
    });
    upload_store::insert(job.clone()).await?;
    upload_store::watch(conn.clone(), job.clone());
    tracing::info!(upload_id = %job.input.upload_id, category = job.input.category,
        bytes = job.input.expected_bytes, "[knowledge] upload started");
    Ok(job.snapshot().await)
}

pub async fn append(
    conn: &DatabaseConnection,
    input: AppendUpload,
) -> Result<UploadStatus, AppCommandError> {
    if input.data_base64.len() > CHUNK_BYTES.div_ceil(3) * 4 {
        return Err(AppCommandError::invalid_input("上传分块超过限制"));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&input.data_base64)
        .map_err(|_| AppCommandError::invalid_input("上传分块编码无效"))?;
    if bytes.is_empty() || bytes.len() > CHUNK_BYTES {
        return Err(AppCommandError::invalid_input("上传分块大小无效"));
    }
    let reference = UploadRef {
        upload_id: input.upload_id,
        scope: input.scope,
    };
    let job = upload_store::lookup(conn, &reference).await?;
    job.monitor.require_current().await?;
    let mut state = job.state.lock().await;
    let next = input.offset.saturating_add(bytes.len() as u64);
    if state.phase != UploadPhase::Receiving
        || state.received != input.offset
        || next > job.input.expected_bytes
        || job.cancel.is_cancelled()
    {
        return Err(AppCommandError::new(
            AppErrorCode::Conflict,
            "上传进度已变更，请重试当前文件",
        ));
    }
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(&job.path)
        .await
        .map_err(upload_store::io_error)?;
    // 明确按已确认 offset 写入，失败后的同一分块重试不会重复追加。
    tokio::io::AsyncSeekExt::seek(&mut file, std::io::SeekFrom::Start(input.offset))
        .await
        .map_err(upload_store::io_error)?;
    file.write_all(&bytes)
        .await
        .map_err(upload_store::io_error)?;
    file.flush().await.map_err(upload_store::io_error)?;
    state.received = next;
    state.touched = Instant::now();
    drop(state);
    Ok(job.snapshot().await)
}

pub async fn start(
    conn: &DatabaseConnection,
    input: UploadRef,
) -> Result<UploadStatus, AppCommandError> {
    let job = upload_store::lookup(conn, &input).await?;
    job.monitor.require_current().await?;
    let mut state = job.state.lock().await;
    if matches!(state.phase, UploadPhase::Receiving | UploadPhase::Failed) {
        if state.received != job.input.expected_bytes || job.cancel.is_cancelled() {
            return Err(AppCommandError::invalid_input("文件尚未传输完整，请重试"));
        }
        state.phase = if state.object.is_some() {
            UploadPhase::Registering
        } else {
            UploadPhase::Uploading
        };
        state.error = None;
        state.touched = Instant::now();
        let conn = conn.clone();
        let job = job.clone();
        tokio::spawn(async move {
            super::upload_worker::run(conn, job).await;
        });
    }
    drop(state);
    Ok(job.snapshot().await)
}

pub async fn status(
    conn: &DatabaseConnection,
    input: UploadRef,
) -> Result<UploadStatus, AppCommandError> {
    Ok(upload_store::lookup(conn, &input).await?.snapshot().await)
}

pub async fn abort(conn: &DatabaseConnection, input: UploadRef) -> Result<(), AppCommandError> {
    library::require_scope(conn, &input.scope).await?;
    let mut uploads = upload_store::store().lock().await;
    if uploads
        .get(&input.upload_id)
        .is_some_and(|job| job.input.scope != input.scope)
    {
        return Err(AppCommandError::permission_denied("无权取消此上传"));
    }
    if let Some(job) = uploads.remove(&input.upload_id) {
        job.cancel.cancel();
        tracing::info!(upload_id = %input.upload_id, "[knowledge] upload cancelled or released");
    }
    Ok(())
}

fn contains(folders: &[super::models::Folder], id: &str) -> bool {
    folders
        .iter()
        .any(|folder| folder.id == id || contains(&folder.children, id))
}
