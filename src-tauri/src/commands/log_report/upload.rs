use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::{header, Method, Url};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::io::ReaderStream;

use super::{archive::PreparedReport, ReportResult};
use crate::app_error::AppCommandError;
use crate::commands::skill_market::client;

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const PURPOSE: &str = "diagnostic_report";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadTicket {
    file_url: String,
    upload: UploadTarget,
}

#[derive(Deserialize)]
struct UploadTarget {
    url: String,
    method: String,
    headers: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Envelope {
    code: i32,
    data: Value,
}

pub(super) async fn send(
    conn: &DatabaseConnection,
    report: PreparedReport,
) -> Result<ReportResult, AppCommandError> {
    let ticket = initialize(conn, &report).await?;
    put_archive(report.file, &ticket.upload, report.size_bytes).await?;
    tracing::info!(date = %report.date, size_bytes = report.size_bytes, "[log-report] upload completed");
    Ok(ReportResult {
        file_url: ticket.file_url,
        date: report.date,
        size_bytes: report.size_bytes,
        log_records: report.log_records,
        skipped_records: report.skipped_records,
    })
}

fn storage_request(
    target: &UploadTarget,
    size_bytes: u64,
) -> Result<reqwest::RequestBuilder, AppCommandError> {
    let mut request = client::http_client()?
        .put(&target.url)
        .timeout(UPLOAD_TIMEOUT)
        .header(header::CONTENT_LENGTH, size_bytes);
    for (name, value) in &target.headers {
        let name = header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| invalid_ticket())?;
        if matches!(
            name.as_str(),
            "content-length"
                | "host"
                | "connection"
                | "transfer-encoding"
                | "authorization"
                | "token"
                | "cookie"
        ) {
            return Err(invalid_ticket());
        }
        let value = header::HeaderValue::from_str(value).map_err(|_| invalid_ticket())?;
        request = request.header(name, value);
    }
    Ok(request)
}

async fn put_archive(
    file: std::fs::File,
    target: &UploadTarget,
    size_bytes: u64,
) -> Result<(), AppCommandError> {
    tracing::info!(size_bytes, "[log-report] uploading archive");
    let file = tokio::fs::File::from_std(file);
    let response = storage_request(target, size_bytes)?
        .body(reqwest::Body::wrap_stream(ReaderStream::new(file)))
        .send()
        .await
        .map_err(|error| {
            network("Archive upload failed", "uploadFailed")
                .with_detail(error.without_url().to_string())
        })?;
    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "[log-report] storage rejected archive");
        return Err(network("Storage rejected the log report", "uploadFailed")
            .with_detail(response.status().to_string()));
    }
    Ok(())
}

async fn initialize(
    conn: &DatabaseConnection,
    report: &PreparedReport,
) -> Result<UploadTicket, AppCommandError> {
    let response = client::request(conn, Method::POST, "/v1/uploads/init").await?
        .json(&json!({"purpose": PURPOSE, "fileName": format!("log-report-{}.zip", report.date),
            "contentType": "application/zip", "sizeBytes": report.size_bytes, "sha256": report.sha256}))
        .send().await.map_err(|error| network("Report upload initialization failed", "initializeFailed")
            .with_detail(error.without_url().to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(network(
            "Report upload initialization was rejected",
            "initializeFailed",
        )
        .with_detail(status.to_string()));
    }
    let envelope = response
        .json::<Envelope>()
        .await
        .map_err(|_| invalid_ticket())?;
    if envelope.code != 1 {
        let error_code = envelope
            .data
            .get("errorCode")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        tracing::warn!(
            business_code = envelope.code,
            error_code,
            "[log-report] Fusion rejected initialization"
        );
        return Err(network(
            "Fusion rejected report upload initialization",
            "initializeFailed",
        ));
    }
    let ticket: UploadTicket =
        serde_json::from_value(envelope.data).map_err(|_| invalid_ticket())?;
    validate_ticket(&ticket)?;
    Ok(ticket)
}

fn validate_ticket(ticket: &UploadTicket) -> Result<(), AppCommandError> {
    let file = secure_url(&ticket.file_url)?;
    let upload = secure_url(&ticket.upload.url)?;
    if file.query().is_some()
        || !ticket.upload.method.eq_ignore_ascii_case("PUT")
        || file.origin() != upload.origin()
        || file.path() != upload.path()
        || !file.path().contains("/uploads/diagnostic_report/")
        || !file.path().ends_with(".zip")
    {
        return Err(invalid_ticket());
    }
    Ok(())
}

fn secure_url(value: &str) -> Result<Url, AppCommandError> {
    let url = Url::parse(value).map_err(|_| invalid_ticket())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid_ticket());
    }
    Ok(url)
}

fn invalid_ticket() -> AppCommandError {
    network(
        "Fusion returned invalid upload information",
        "initializeFailed",
    )
}

fn network(message: &str, key: &str) -> AppCommandError {
    AppCommandError::network(message)
        .with_i18n(format!("LogReport.errors.{key}"), Default::default())
}
