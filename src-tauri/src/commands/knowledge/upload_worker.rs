use std::sync::{atomic::Ordering, Arc, OnceLock};
use std::time::Duration;

use futures::TryStreamExt;
use sea_orm::DatabaseConnection;
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tokio_util::io::ReaderStream;

use super::{
    api::KnowledgeClient,
    library, tos_signing,
    upload_models::*,
    upload_store::{io_error, UploadJob},
};
use crate::app_error::{AppCommandError, AppErrorCode};

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const REGISTER_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_CLOUD_UPLOADS: usize = 2;
const RECONCILE_PAGE_SIZE: usize = 100;
const RECONCILE_MAX_PAGES: usize = 10;
static UPLOAD_PERMITS: Semaphore = Semaphore::const_new(MAX_CLOUD_UPLOADS);

pub async fn run(conn: DatabaseConnection, job: Arc<UploadJob>) {
    let result = tokio::select! {
        biased;
        _ = job.cancel.cancelled() => Err(AppCommandError::task_execution_failed("上传已取消；若已开始登记，请刷新目录确认结果")),
        result = execute(&conn, &job) => result,
    };
    match result {
        Ok(()) => {
            job.transition(UploadPhase::Complete).await;
            tracing::info!(upload_id = %job.input.upload_id, "[knowledge] cloud file registered");
        }
        Err(error) => {
            let mut state = job.state.lock().await;
            state.phase = if job.cancel.is_cancelled() {
                UploadPhase::Cancelled
            } else {
                UploadPhase::Failed
            };
            state.error = Some(error.message);
            tracing::warn!(upload_id = %job.input.upload_id, code = ?error.code,
                "[knowledge] cloud upload or registration failed");
        }
    }
}

async fn execute(conn: &DatabaseConnection, job: &Arc<UploadJob>) -> Result<(), AppCommandError> {
    let _permit = UPLOAD_PERMITS
        .acquire()
        .await
        .map_err(|_| AppCommandError::task_execution_failed("上传服务已关闭"))?;
    job.monitor.require_current().await?;
    let client = library::scoped_client(conn, &job.input.scope).await?;
    let object = job.state.lock().await.object.clone();
    let object = match object {
        Some(object) => object,
        None => {
            let object = upload_object(&client, job).await?;
            job.state.lock().await.object = Some(object.clone());
            object
        }
    };
    library::require_scope(conn, &job.input.scope).await?;
    job.monitor.require_current().await?;
    job.transition(UploadPhase::Registering).await;
    register(&client, job, &object).await?;
    library::require_scope(conn, &job.input.scope).await
}

async fn upload_object(
    client: &KnowledgeClient,
    job: &Arc<UploadJob>,
) -> Result<UploadedObject, AppCommandError> {
    let input = &job.input;
    let credentials = upload_credentials(client, input).await?;
    let (url, object) = tos_signing::object(&credentials, &input.file_name)?;
    let headers = tos_signing::headers(&credentials, &url)?;
    validate_staged(job).await?;
    transfer(job, url, headers).await?;
    tracing::info!(upload_id = %input.upload_id, bytes = input.expected_bytes, "[knowledge] file transferred to cloud storage");
    Ok(object)
}

async fn upload_credentials(
    client: &KnowledgeClient,
    input: &BeginUpload,
) -> Result<UploadCredentials, AppCommandError> {
    let credentials: UploadCredentials = client
        .post(
            "files/upload-token",
            json!({ "category": input.category, "folderId": input.folder_id }),
        )
        .await?;
    if credentials.folder_id != input.folder_id {
        return Err(AppCommandError::permission_denied(
            "上传凭证的目标目录不匹配",
        ));
    }
    Ok(credentials)
}

async fn transfer(
    job: &Arc<UploadJob>,
    url: reqwest::Url,
    headers: reqwest::header::HeaderMap,
) -> Result<(), AppCommandError> {
    let input = &job.input;
    let file = tokio::fs::File::open(&job.path).await.map_err(io_error)?;
    job.sent.store(0, Ordering::Relaxed);
    let progress = job.clone();
    let stream = ReaderStream::new(file).inspect_ok(move |chunk| {
        progress
            .sent
            .fetch_add(chunk.len() as u64, Ordering::Relaxed);
    });
    let response = client_for_upload()?
        .put(url)
        .headers(headers)
        .header(reqwest::header::CONTENT_LENGTH, input.expected_bytes)
        .header(
            reqwest::header::CONTENT_TYPE,
            if input.mime_type.is_empty() {
                "application/octet-stream"
            } else {
                &input.mime_type
            },
        )
        .body(reqwest::Body::wrap_stream(stream))
        .send()
        .await
        .map_err(storage_network_error)?;
    if !response.status().is_success() {
        return Err(
            AppCommandError::network("云存储拒绝上传，请检查空间容量后重试")
                .with_detail(format!("http_status={}", response.status().as_u16())),
        );
    }
    Ok(())
}

fn storage_network_error(error: reqwest::Error) -> AppCommandError {
    AppCommandError::network("文件上传到云端失败，请重试").with_detail(format!(
        "timeout={},connect={}",
        error.is_timeout(),
        error.is_connect()
    ))
}

async fn validate_staged(job: &UploadJob) -> Result<(), AppCommandError> {
    let metadata = tokio::fs::metadata(&job.path).await.map_err(io_error)?;
    if metadata.len() != job.input.expected_bytes {
        return Err(AppCommandError::invalid_input(
            "暂存文件大小不匹配，请重新选择文件",
        ));
    }
    let path = job.path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let reader = image::ImageReader::open(path).map_err(io_error)?;
        let reader = reader.with_guessed_format().map_err(io_error)?;
        // 只读取受支持图片的头部；其他格式交由云端解析，不把文档读入内存。
        let Ok((width, height)) = reader.into_dimensions() else {
            return Ok(());
        };
        let ratio = f64::from(width) / f64::from(height);
        if !(10..=6000).contains(&width)
            || !(10..=6000).contains(&height)
            || !(0.01..=100.0).contains(&ratio)
        {
            return Err(AppCommandError::invalid_input(
                "图片尺寸或宽高比超过云端限制",
            ));
        }
        Ok(())
    })
    .await
    .map_err(|_| AppCommandError::task_execution_failed("无法检查上传文件"))?
}

async fn register(
    client: &KnowledgeClient,
    job: &UploadJob,
    object: &UploadedObject,
) -> Result<(), AppCommandError> {
    if job.state.lock().await.registration_attempted {
        return reconcile(client, &job.input, object).await;
    }
    job.state.lock().await.registration_attempted = true;
    let input = &job.input;
    let result = client
        .post_with_timeout::<Value>(
            "files/create",
            json!({
                "category": input.category, "folderId": input.folder_id, "fileType": 0,
                "uri": object.public_url, "fileUrl": object.public_url,
                "tosUrl": object.tos_url, "tosPath": object.key,
                "fileName": input.file_name, "originalFileName": input.file_name,
                "mimeType": input.mime_type, "fileSize": input.expected_bytes,
            }),
            REGISTER_TIMEOUT,
        )
        .await;
    match result {
        Ok(_) => Ok(()),
        Err(error) => {
            if matches!(
                error.code,
                AppErrorCode::InvalidInput
                    | AppErrorCode::AuthenticationFailed
                    | AppErrorCode::PermissionDenied
            ) {
                job.state.lock().await.registration_attempted = false;
            }
            Err(error)
        }
    }
}

async fn reconcile(
    client: &KnowledgeClient,
    input: &BeginUpload,
    object: &UploadedObject,
) -> Result<(), AppCommandError> {
    for page in 1..=RECONCILE_MAX_PAGES {
        let data: Value = client
            .post(
                "files/list",
                json!({
                    "category": input.category, "folderId": input.folder_id,
                    "searchText": input.file_name, "includeChildren": false,
                    "page": page, "pageSize": RECONCILE_PAGE_SIZE,
                }),
            )
            .await?;
        let list = data["list"]
            .as_array()
            .ok_or_else(super::api::invalid_response)?;
        if list
            .iter()
            .any(|file| file["tosPath"].as_str() == Some(object.key.as_str()))
        {
            return Ok(());
        }
        if list.len() < RECONCILE_PAGE_SIZE {
            break;
        }
    }
    Err(AppCommandError::network(
        "文件已上传，但登记结果尚未确认。请稍后刷新目录并重试确认，避免重复登记",
    ))
}

fn client_for_upload() -> Result<&'static reqwest::Client, AppCommandError> {
    static CLIENT: OnceLock<Result<reqwest::Client, reqwest::Error>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .https_only(true)
                .connect_timeout(Duration::from_secs(5))
                .timeout(UPLOAD_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
        })
        .as_ref()
        .map_err(|_| AppCommandError::network("无法初始化云端上传"))
}
