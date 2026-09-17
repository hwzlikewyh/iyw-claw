use std::sync::Arc;

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use crate::app_error::AppCommandError;
use crate::logging::beijing;

mod archive;
mod files;
mod screenshots;
mod upload;

pub const MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;
const MAX_DESCRIPTION_CHARS: usize = 2000;
static UPLOAD_SLOT: Semaphore = Semaphore::const_new(1);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportRequest {
    pub date: String,
    pub description: String,
    #[serde(default)]
    pub screenshots: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportContext {
    pub today: String,
    pub date: String,
    pub logs_dir: String,
    pub source_files: Vec<SourceFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    pub name: String,
    pub size_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportResult {
    pub file_url: String,
    pub date: String,
    pub size_bytes: u64,
    pub log_records: u64,
    pub skipped_records: u64,
}

pub async fn get_log_report_context_core(
    date: Option<String>,
) -> Result<ReportContext, AppCommandError> {
    tokio::task::spawn_blocking(move || {
        let today = beijing::now().date_naive().to_string();
        let selected = files::parse_date(date.as_deref().unwrap_or(&today))?;
        let source_files = files::sources(selected)?
            .into_iter()
            .map(|file| SourceFile {
                name: file.name,
                size_bytes: file.size_bytes,
            })
            .collect();
        Ok(ReportContext {
            today,
            date: selected.to_string(),
            source_files,
            logs_dir: crate::paths::iyw_claw_logs_root()
                .to_string_lossy()
                .into_owned(),
        })
    })
    .await
    .map_err(|error| io_error("Failed to inspect report logs", error))?
}

pub async fn submit_log_report_core(
    conn: &DatabaseConnection,
    request: ReportRequest,
) -> Result<ReportResult, AppCommandError> {
    let _permit = UPLOAD_SLOT.try_acquire().map_err(|_| {
        AppCommandError::new(
            crate::app_error::AppErrorCode::Conflict,
            "A log report is already being uploaded",
        )
        .with_i18n("LogReport.errors.busy", Default::default())
    })?;
    validate_request(&request)?;
    let token = crate::commands::iyw_account::iyw_account_access_token_core(conn).await?;
    if token.is_none() {
        return Err(
            AppCommandError::authentication_failed("Sign in before reporting logs")
                .with_i18n("LogReport.errors.signIn", Default::default()),
        );
    }
    drop(token);
    tracing::info!(date = %request.date, screenshots = request.screenshots.len(), "[log-report] preparing archive");
    let retained_permit = Arc::new(_permit);
    let worker_permit = retained_permit.clone();
    let prepared = tokio::task::spawn_blocking(move || {
        let _permit = worker_permit;
        archive::prepare(request)
    })
    .await
    .map_err(|error| io_error("Log report preparation failed", error))?
    .map_err(|error| {
        tracing::warn!(code = ?error.code, reason = %error.message, detail = ?error.detail, "[log-report] archive preparation failed");
        error
    })?;
    let result = upload::send(conn, prepared).await;
    if let Err(error) = &result {
        tracing::warn!(code = ?error.code, reason = %error.message, "[log-report] submission failed");
    }
    result
}

fn validate_request(request: &ReportRequest) -> Result<(), AppCommandError> {
    files::parse_date(&request.date)?;
    let length = request.description.trim().chars().count();
    if length == 0 || length > MAX_DESCRIPTION_CHARS {
        return Err(invalid(
            "Provide a problem description of at most 2000 characters",
            "description",
        ));
    }
    screenshots::validate_encoded(&request.screenshots)
}

fn invalid(message: &str, key: &str) -> AppCommandError {
    AppCommandError::invalid_input(message)
        .with_i18n(format!("LogReport.errors.{key}"), Default::default())
}

fn io_error(message: &str, error: impl std::fmt::Display) -> AppCommandError {
    AppCommandError::io_error(message)
        .with_detail(error.to_string())
        .with_i18n("LogReport.errors.readFailed", Default::default())
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_log_report_context(
    date: Option<String>,
) -> Result<ReportContext, AppCommandError> {
    get_log_report_context_core(date).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn submit_log_report(
    db: tauri::State<'_, crate::db::AppDatabase>,
    request: ReportRequest,
) -> Result<ReportResult, AppCommandError> {
    submit_log_report_core(&db.conn, request).await
}
