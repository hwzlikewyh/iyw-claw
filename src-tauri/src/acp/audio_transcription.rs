use std::fs::File;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::acp::delegation::transport::{
    BrokerAudioTranscriptionQueryRequest, BrokerAudioTranscriptionRequest,
};
use crate::db::AppDatabase;

#[path = "audio_transcription_client.rs"]
mod audio_transcription_client;

const TRANSCRIPTION_WAIT_WINDOW: Duration = Duration::from_secs(45);
const TRANSCRIPTION_QUERY_INTERVAL: Duration = Duration::from_secs(3);

pub(crate) struct PreparedAudioFile {
    pub(crate) file: tokio::fs::File,
    pub(crate) file_name: String,
    pub(crate) content_type: &'static str,
    pub(crate) size_bytes: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct AudioToolFailure {
    code: &'static str,
    message: &'static str,
}

impl AudioToolFailure {
    pub(crate) fn invalid_path() -> Self {
        Self {
            code: "audio_transcription_invalid_path",
            message: "Audio path must be a readable workspace-relative audio file.",
        }
    }

    pub(crate) fn invalid_response() -> Self {
        Self {
            code: "audio_transcription_invalid_response",
            message: "The transcription service returned an invalid response.",
        }
    }

    fn invalid_arguments() -> Self {
        Self {
            code: "audio_transcription_invalid_arguments",
            message: "Audio transcription arguments are invalid.",
        }
    }

    pub(crate) fn authentication_required() -> Self {
        Self {
            code: "audio_transcription_auth_required",
            message: "Sign in to iyw-claw before transcribing audio.",
        }
    }

    pub(crate) fn transport() -> Self {
        Self {
            code: "audio_transcription_transport_failed",
            message: "The transcription service could not be reached.",
        }
    }

    pub(crate) fn upload_failed() -> Self {
        Self {
            code: "audio_transcription_upload_failed",
            message: "The audio file could not be uploaded.",
        }
    }

    pub(crate) fn request_failed() -> Self {
        Self {
            code: "audio_transcription_request_failed",
            message: "The transcription request was rejected.",
        }
    }
}

#[async_trait]
pub trait AudioTranscriptionAccess: Send + Sync {
    async fn transcribe(
        &self,
        working_dir: &Path,
        request: BrokerAudioTranscriptionRequest,
    ) -> Value;
    async fn query(&self, request: BrokerAudioTranscriptionQueryRequest) -> Value;
}

pub struct HostAudioTranscriptionService {
    db: Arc<AppDatabase>,
}

impl HostAudioTranscriptionService {
    pub fn new(db: Arc<AppDatabase>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl AudioTranscriptionAccess for HostAudioTranscriptionService {
    async fn transcribe(
        &self,
        working_dir: &Path,
        request: BrokerAudioTranscriptionRequest,
    ) -> Value {
        let language = match normalize_language(request.language.as_deref()) {
            Ok(language) => language,
            Err(error) => return error_result(error),
        };
        let file = match prepare_audio_file(working_dir, &request.path).await {
            Ok(file) => file,
            Err(error) => return error_result(error),
        };
        tracing::info!(file_name = %file.file_name, size_bytes = file.size_bytes, "[AudioTranscription] uploading audio file");
        let ticket = match audio_transcription_client::initialize(&self.db.conn, &file).await {
            Ok(ticket) => ticket,
            Err(error) => return error_result(error),
        };
        if let Err(error) = audio_transcription_client::upload(file, &ticket).await {
            return error_result(error);
        }
        let job = match audio_transcription_client::submit(
            &self.db.conn,
            ticket,
            &language,
            request.options,
        )
        .await
        {
            Ok(job) => job,
            Err(error) => return error_result(error),
        };
        wait_for_completion(&self.db.conn, job).await
    }

    async fn query(&self, request: BrokerAudioTranscriptionQueryRequest) -> Value {
        match audio_transcription_client::query(&self.db.conn, &request.job_id).await {
            Ok(job) => render_job(job),
            Err(error) => error_result(error),
        }
    }
}

async fn prepare_audio_file(
    working_dir: &Path,
    source: &str,
) -> Result<PreparedAudioFile, AudioToolFailure> {
    let working_dir = working_dir.to_path_buf();
    let source = source.to_string();
    tokio::task::spawn_blocking(move || open_audio_file(&working_dir, &source))
        .await
        .map_err(|_| AudioToolFailure::invalid_path())?
}

fn open_audio_file(
    working_dir: &Path,
    source: &str,
) -> Result<PreparedAudioFile, AudioToolFailure> {
    let relative = parse_relative_path(source)?;
    let root = std::fs::canonicalize(working_dir).map_err(|_| AudioToolFailure::invalid_path())?;
    let path =
        std::fs::canonicalize(root.join(relative)).map_err(|_| AudioToolFailure::invalid_path())?;
    if !path.starts_with(&root) {
        return Err(AudioToolFailure::invalid_path());
    }
    let file = File::open(&path).map_err(|_| AudioToolFailure::invalid_path())?;
    let metadata = file
        .metadata()
        .map_err(|_| AudioToolFailure::invalid_path())?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(AudioToolFailure::invalid_path());
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(AudioToolFailure::invalid_path)?
        .to_string();
    let content_type = audio_content_type(&path).ok_or_else(AudioToolFailure::invalid_path)?;
    Ok(PreparedAudioFile {
        file: tokio::fs::File::from_std(file),
        file_name,
        content_type,
        size_bytes: metadata.len(),
    })
}

fn parse_relative_path(source: &str) -> Result<PathBuf, AudioToolFailure> {
    let path = Path::new(source.trim());
    let valid = !source.trim().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    valid
        .then(|| path.to_path_buf())
        .ok_or_else(AudioToolFailure::invalid_path)
}

fn audio_content_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "mp3" => Some("audio/mpeg"),
        "wav" => Some("audio/wav"),
        "ogg" => Some("audio/ogg"),
        "mp4" => Some("audio/mp4"),
        "pcm" => Some("audio/pcm"),
        _ => None,
    }
}

fn normalize_language(value: Option<&str>) -> Result<String, AudioToolFailure> {
    let language = value.unwrap_or("zh-CN").trim();
    let valid = !language.is_empty()
        && language.chars().count() <= 35
        && language
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    valid
        .then(|| language.to_string())
        .ok_or_else(AudioToolFailure::invalid_arguments)
}

async fn wait_for_completion(
    conn: &sea_orm::DatabaseConnection,
    mut job: audio_transcription_client::JobResult,
) -> Value {
    let started = Instant::now();
    while !job.status.is_terminal() {
        let remaining = TRANSCRIPTION_WAIT_WINDOW.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        tokio::time::sleep(TRANSCRIPTION_QUERY_INTERVAL.min(remaining)).await;
        let remaining = TRANSCRIPTION_WAIT_WINDOW.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        job = match tokio::time::timeout(
            remaining,
            audio_transcription_client::query(conn, &job.job_id),
        )
        .await
        {
            Ok(Ok(next)) => next,
            Ok(Err(error)) => return error_result(error),
            Err(_) => break,
        };
    }
    render_job(job)
}

fn render_job(job: audio_transcription_client::JobResult) -> Value {
    let is_error = job.status.is_error();
    tracing::info!(job_id = %job.job_id, status = job.status.as_str(), "[AudioTranscription] transcription state received");
    json!({
        "content": [{ "type": "text", "text": job_summary(&job) }],
        "isError": is_error,
        "structuredContent": {
            "jobId": job.job_id,
            "status": job.status.as_str(),
            "transcript": job.transcript,
            "errorCode": job.error_code,
            "errorMessage": job.error_message,
        },
    })
}

fn job_summary(job: &audio_transcription_client::JobResult) -> String {
    match &job.transcript {
        Some(transcript) if !transcript.text.trim().is_empty() => transcript.text.clone(),
        _ => format!(
            "Transcription job {} is {}.",
            job.job_id,
            job.status.as_str()
        ),
    }
}

fn error_result(error: AudioToolFailure) -> Value {
    json!({
        "content": [{ "type": "text", "text": error.message }],
        "isError": true,
        "structuredContent": { "code": error.code, "error": error.message },
    })
}
