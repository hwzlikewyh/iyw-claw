use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;

use reqwest::header::{HeaderName, HeaderValue, CONTENT_LENGTH};
use reqwest::{Method, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::io::ReaderStream;

use super::audio_transcription::{AudioToolFailure, PreparedAudioFile};
use crate::acp::delegation::transport::AudioTranscriptionOptions;
use crate::commands::skill_market::client as fusion_client;

const API_TIMEOUT: Duration = Duration::from_secs(60);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub(crate) struct UploadTicket {
    file_url: String,
    upload: UploadTarget,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadInitResponse {
    file_url: String,
    upload: UploadTarget,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadTarget {
    url: String,
    method: String,
    headers: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct FusionEnvelope {
    code: i32,
    data: Value,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TranscriptionStatus {
    Queued,
    Processing,
    Succeeded,
    Failed,
    Expired,
    Cancelled,
}

impl TranscriptionStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Processing => "processing",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Expired | Self::Cancelled
        )
    }

    pub(crate) fn is_error(self) -> bool {
        matches!(self, Self::Failed | Self::Expired | Self::Cancelled)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobResult {
    pub(crate) job_id: String,
    pub(crate) status: TranscriptionStatus,
    #[serde(default)]
    pub(crate) transcript: Option<Transcript>,
    #[serde(default)]
    pub(crate) error_code: Option<String>,
    #[serde(default)]
    pub(crate) error_message: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Transcript {
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) language: Option<String>,
    #[serde(default)]
    pub(crate) duration_ms: Option<u64>,
    #[serde(default)]
    pub(crate) segments: Vec<TranscriptSegment>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranscriptSegment {
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) speaker: Option<String>,
    #[serde(default)]
    pub(crate) channel: Option<i64>,
    #[serde(default)]
    pub(crate) words: Vec<TranscriptWord>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranscriptWord {
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) text: String,
}

pub(crate) async fn initialize(
    conn: &sea_orm::DatabaseConnection,
    file: &PreparedAudioFile,
) -> Result<UploadTicket, AudioToolFailure> {
    let data = post(
        conn,
        "/v1/uploads/init",
        json!({
            "purpose": "voice_transcription",
            "fileName": file.file_name,
            "contentType": file.content_type,
            "sizeBytes": file.size_bytes,
        }),
    )
    .await?;
    let response = serde_json::from_value::<UploadInitResponse>(data)
        .map_err(|_| AudioToolFailure::invalid_response())?;
    validate_ticket(&response)?;
    Ok(UploadTicket {
        file_url: response.file_url,
        upload: response.upload,
    })
}

pub(crate) async fn upload(
    file: PreparedAudioFile,
    ticket: &UploadTicket,
) -> Result<(), AudioToolFailure> {
    let target = &ticket.upload;
    let mut request = fusion_client::http_client()
        .map_err(|_| AudioToolFailure::transport())?
        .put(&target.url)
        .timeout(UPLOAD_TIMEOUT)
        .header(CONTENT_LENGTH, file.size_bytes);
    for (name, value) in &target.headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| AudioToolFailure::invalid_response())?;
        if matches!(
            name.as_str(),
            "content-length" | "host" | "connection" | "transfer-encoding"
        ) {
            return Err(AudioToolFailure::invalid_response());
        }
        let value = HeaderValue::from_str(value.as_str())
            .map_err(|_| AudioToolFailure::invalid_response())?;
        request = request.header(name, value);
    }
    let response = request
        .body(reqwest::Body::wrap_stream(ReaderStream::new(file.file)))
        .send()
        .await
        .map_err(|_| AudioToolFailure::upload_failed())?;
    if response.status().is_success() {
        Ok(())
    } else {
        tracing::warn!(status = %response.status(), "[AudioTranscription] TOS upload rejected");
        Err(AudioToolFailure::upload_failed())
    }
}

pub(crate) async fn submit(
    conn: &sea_orm::DatabaseConnection,
    ticket: UploadTicket,
    language: &str,
    options: AudioTranscriptionOptions,
) -> Result<JobResult, AudioToolFailure> {
    let data = post(
        conn,
        "/v1/voice/transcriptions/submit",
        json!({
            "fileUrl": ticket.file_url,
            "language": language,
            "options": options,
        }),
    )
    .await?;
    parse_job(data)
}

pub(crate) async fn query(
    conn: &sea_orm::DatabaseConnection,
    job_id: &str,
) -> Result<JobResult, AudioToolFailure> {
    let data = post(
        conn,
        "/v1/voice/transcriptions/query",
        json!({ "jobId": job_id.trim() }),
    )
    .await?;
    parse_job(data)
}

async fn post(
    conn: &sea_orm::DatabaseConnection,
    path: &str,
    body: Value,
) -> Result<Value, AudioToolFailure> {
    let request = fusion_client::request(conn, Method::POST, path)
        .await
        .map_err(|_| AudioToolFailure::authentication_required())?
        .timeout(API_TIMEOUT)
        .json(&body);
    send_gateway(request).await
}

async fn send_gateway(request: RequestBuilder) -> Result<Value, AudioToolFailure> {
    let response = request
        .send()
        .await
        .map_err(|_| AudioToolFailure::transport())?;
    let status = response.status();
    let envelope = response
        .json::<FusionEnvelope>()
        .await
        .map_err(|_| AudioToolFailure::invalid_response())?;
    if !status.is_success() {
        tracing::warn!(status = %status, "[AudioTranscription] Fusion request rejected");
        return Err(AudioToolFailure::request_failed());
    }
    if envelope.code == 1 {
        Ok(envelope.data)
    } else {
        Err(AudioToolFailure::request_failed())
    }
}

fn validate_ticket(response: &UploadInitResponse) -> Result<(), AudioToolFailure> {
    let file_url = Url::parse(&response.file_url).ok();
    let file_url_valid = file_url.is_some_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    });
    let upload_url_valid = Url::parse(&response.upload.url).ok().is_some_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
    });
    if file_url_valid && upload_url_valid && response.upload.method.eq_ignore_ascii_case("PUT") {
        Ok(())
    } else {
        Err(AudioToolFailure::invalid_response())
    }
}

fn parse_job(data: Value) -> Result<JobResult, AudioToolFailure> {
    let job = serde_json::from_value::<JobResult>(data)
        .map_err(|_| AudioToolFailure::invalid_response())?;
    let valid_id = !job.job_id.is_empty()
        && job.job_id.len() <= 32
        && job.job_id.bytes().all(|byte| byte.is_ascii_digit());
    if valid_id {
        Ok(job)
    } else {
        Err(AudioToolFailure::invalid_response())
    }
}
