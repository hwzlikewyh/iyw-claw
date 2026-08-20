use std::time::Instant;

use serde_json::{json, Value};

use super::{
    audio_transcription_client, AudioToolFailure, TRANSCRIPTION_QUERY_INTERVAL,
    TRANSCRIPTION_WAIT_WINDOW,
};

pub(super) async fn wait_for_completion(
    conn: &sea_orm::DatabaseConnection,
    mut value: audio_transcription_client::JobResult,
) -> Value {
    let started = Instant::now();
    while !value.status.is_terminal() {
        let remaining = TRANSCRIPTION_WAIT_WINDOW.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        tokio::time::sleep(TRANSCRIPTION_QUERY_INTERVAL.min(remaining)).await;
        let remaining = TRANSCRIPTION_WAIT_WINDOW.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        value = match tokio::time::timeout(
            remaining,
            audio_transcription_client::query(conn, &value.job_id),
        )
        .await
        {
            Ok(Ok(next)) => next,
            Ok(Err(error)) => return error_result(error),
            Err(_) => break,
        };
    }
    job(value)
}

pub(super) fn job(value: audio_transcription_client::JobResult) -> Value {
    let is_error = value.status.is_error();
    tracing::info!(
        job_id = %value.job_id,
        status = value.status.as_str(),
        "[AudioTranscription] transcription state received"
    );
    json!({
        "content": [{ "type": "text", "text": job_summary(&value) }],
        "isError": is_error,
        "structuredContent": {
            "jobId": value.job_id,
            "status": value.status.as_str(),
            "transcript": value.transcript,
            "errorCode": value.error_code,
            "errorMessage": value.error_message,
        },
    })
}

pub(super) fn flash(transcript: audio_transcription_client::Transcript) -> Value {
    let text = transcript.text.clone();
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": {
            "status": "succeeded",
            "transcript": transcript,
        },
    })
}

fn job_summary(value: &audio_transcription_client::JobResult) -> String {
    match &value.transcript {
        Some(transcript) if !transcript.text.trim().is_empty() => transcript.text.clone(),
        _ => format!(
            "Transcription job {} is {}.",
            value.job_id,
            value.status.as_str()
        ),
    }
}

pub(super) fn error_result(error: AudioToolFailure) -> Value {
    json!({
        "content": [{ "type": "text", "text": error.message }],
        "isError": true,
        "structuredContent": { "code": error.code, "error": error.message },
    })
}

pub(super) fn capability_error_result(error: &crate::app_error::AppCommandError) -> Value {
    json!({
        "content": [{ "type": "text", "text": "Audio transcription is disabled by capability policy." }],
        "isError": true,
        "structuredContent": {
            "code": error.detail.as_deref().unwrap_or("remote_policy_denied"),
            "error": "Audio transcription is disabled by capability policy.",
        },
    })
}
