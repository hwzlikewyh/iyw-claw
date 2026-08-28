use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::acp::audio_source::{self, PreparedAudioFile};
use crate::acp::delegation::transport::{
    BrokerAudioTranscriptionQueryRequest, BrokerAudioTranscriptionRequest,
};
use crate::db::AppDatabase;

#[path = "audio_transcription_client.rs"]
mod audio_transcription_client;
#[path = "audio_transcription_error.rs"]
mod failure;
#[path = "audio_transcription_render.rs"]
mod render;

pub(crate) use failure::AudioToolFailure;

const TRANSCRIPTION_WAIT_WINDOW: Duration = Duration::from_secs(45);
const TRANSCRIPTION_QUERY_INTERVAL: Duration = Duration::from_secs(3);
const STANDARD_MAX_BYTES: u64 = (512 << 20) - 1;
const FLASH_MAX_BYTES: u64 = 95 << 20;

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

    async fn upload(
        &self,
        monitor: &crate::acp::capability_policy::CapabilityRevocationMonitor,
        file: PreparedAudioFile,
    ) -> Result<audio_transcription_client::UploadTicket, Value> {
        let ticket = monitor
            .run_until_revoked(audio_transcription_client::initialize(&self.db.conn, &file))
            .await
            .map_err(|error| render::capability_error_result(&error))?
            .map_err(render::error_result)?;
        monitor
            .run_until_revoked(audio_transcription_client::upload(file, &ticket))
            .await
            .map_err(|error| render::capability_error_result(&error))?
            .map_err(render::error_result)?;
        Ok(ticket)
    }
}

#[async_trait]
impl AudioTranscriptionAccess for HostAudioTranscriptionService {
    async fn transcribe(
        &self,
        working_dir: &Path,
        request: BrokerAudioTranscriptionRequest,
    ) -> Value {
        let monitor = match crate::acp::capability_policy::monitor_file_upload(None).await {
            Ok(monitor) => monitor,
            Err(error) => return render::capability_error_result(&error),
        };
        let language = match normalize_language(request.language.as_deref()) {
            Ok(language) => language,
            Err(error) => return render::error_result(error),
        };
        let max_bytes = if request.flash {
            FLASH_MAX_BYTES
        } else {
            STANDARD_MAX_BYTES
        };
        let file = match monitor
            .run_until_revoked(audio_source::prepare(
                working_dir,
                &request.source,
                max_bytes,
                request.flash,
            ))
            .await
        {
            Ok(Ok(file)) => file,
            Ok(Err(error)) => return render::error_result(error),
            Err(error) => return render::capability_error_result(&error),
        };
        tracing::info!(
            file_name = %file.file_name,
            size_bytes = file.size_bytes,
            flash = request.flash,
            "[AudioTranscription] uploading normalized audio"
        );
        let ticket = match self.upload(&monitor, file).await {
            Ok(ticket) => ticket,
            Err(result) => return result,
        };
        if request.flash {
            return match monitor
                .run_until_revoked(audio_transcription_client::flash(
                    &self.db.conn,
                    ticket,
                    &language,
                    request.options,
                ))
                .await
            {
                Ok(Ok(transcript)) => render::flash(transcript),
                Ok(Err(error)) => render::error_result(error),
                Err(error) => render::capability_error_result(&error),
            };
        }
        let job = match monitor
            .run_until_revoked(audio_transcription_client::submit(
                &self.db.conn,
                ticket,
                &language,
                request.options,
            ))
            .await
        {
            Ok(Ok(job)) => job,
            Ok(Err(error)) => return render::error_result(error),
            Err(error) => return render::capability_error_result(&error),
        };
        match monitor
            .run_until_revoked(render::wait_for_completion(&self.db.conn, job))
            .await
        {
            Ok(result) => result,
            Err(error) => render::capability_error_result(&error),
        }
    }

    async fn query(&self, request: BrokerAudioTranscriptionQueryRequest) -> Value {
        match audio_transcription_client::query(&self.db.conn, &request.job_id).await {
            Ok(job) => render::job(job),
            Err(error) => render::error_result(error),
        }
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
