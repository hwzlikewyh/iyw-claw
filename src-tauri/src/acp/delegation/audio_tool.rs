use std::path::{Component, Path};

use serde::Deserialize;
use serde_json::{json, Value};

use super::transport::AudioTranscriptionOptions;

const MAX_LANGUAGE_CHARS: usize = 35;
const MAX_JOB_ID_CHARS: usize = 32;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscribeAudioArguments {
    pub path: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub options: AudioTranscriptionOptions,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryAudioTranscriptionArguments {
    pub job_id: String,
}

pub fn prepare_transcribe(arguments: Value) -> Result<TranscribeAudioArguments, Value> {
    let input = serde_json::from_value(arguments).map_err(|_| {
        error_result(
            "audio_transcription_invalid_arguments",
            "Audio transcription arguments are invalid.",
        )
    })?;
    validate_relative_path(&input.path)?;
    if let Some(language) = &input.language {
        validate_language(language)?;
    }
    Ok(input)
}

pub fn prepare_query(arguments: Value) -> Result<QueryAudioTranscriptionArguments, Value> {
    let mut input: QueryAudioTranscriptionArguments =
        serde_json::from_value(arguments).map_err(|_| {
            error_result(
                "audio_transcription_invalid_arguments",
                "Audio transcription arguments are invalid.",
            )
        })?;
    validate_job_id(&input.job_id)?;
    input.job_id = input.job_id.trim().to_string();
    Ok(input)
}

pub fn error_result(code: &str, message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
        "structuredContent": { "code": code, "error": message },
    })
}

fn validate_relative_path(value: &str) -> Result<(), Value> {
    let path = Path::new(value.trim());
    let valid = !value.trim().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    if valid {
        Ok(())
    } else {
        Err(error_result(
            "audio_transcription_invalid_path",
            "Audio path must be a workspace-relative file path.",
        ))
    }
}

fn validate_language(value: &str) -> Result<(), Value> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.chars().count() <= MAX_LANGUAGE_CHARS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(error_result(
            "audio_transcription_invalid_arguments",
            "Language must be a short BCP-47 style tag.",
        ))
    }
}

fn validate_job_id(value: &str) -> Result<(), Value> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= MAX_JOB_ID_CHARS
        && value.bytes().all(|byte| byte.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(error_result(
            "audio_transcription_invalid_job_id",
            "Job ID must be a decimal string.",
        ))
    }
}
