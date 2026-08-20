use std::path::{Component, Path};

use serde::Deserialize;
use serde_json::{json, Value};

use super::transport::{AudioTranscriptionOptions, AudioTranscriptionSource};

const MAX_LANGUAGE_CHARS: usize = 35;
const MAX_JOB_ID_CHARS: usize = 32;
const MAX_INLINE_DATA_CHARS: usize = 24 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscribeAudioArguments {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default, rename = "fileName")]
    pub file_name: Option<String>,
    #[serde(default, rename = "mimeType")]
    pub mime_type: Option<String>,
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
    let input: TranscribeAudioArguments = serde_json::from_value(arguments).map_err(|_| {
        error_result(
            "audio_transcription_invalid_arguments",
            "Audio transcription arguments are invalid.",
        )
    })?;
    validate_source(&input)?;
    if let Some(language) = &input.language {
        validate_language(language)?;
    }
    Ok(input)
}

fn validate_source(input: &TranscribeAudioArguments) -> Result<(), Value> {
    validate_source_count(input)?;
    validate_inline_data(input)?;
    validate_path(input.path.as_deref())?;
    validate_url(input.url.as_deref())?;
    validate_file_name(input.file_name.as_deref())?;
    Ok(())
}

fn validate_source_count(input: &TranscribeAudioArguments) -> Result<(), Value> {
    let source_count = [&input.path, &input.url, &input.data]
        .into_iter()
        .filter(|value| value.is_some())
        .count();
    if source_count != 1 {
        return Err(error_result(
            "audio_transcription_invalid_source",
            "Provide exactly one non-empty path, url, or data source.",
        ));
    }
    if [&input.path, &input.url, &input.data]
        .into_iter()
        .any(|value| value.as_ref().is_some_and(|value| value.trim().is_empty()))
    {
        return Err(error_result(
            "audio_transcription_invalid_source",
            "The selected audio source must not be empty.",
        ));
    }
    Ok(())
}

fn validate_inline_data(input: &TranscribeAudioArguments) -> Result<(), Value> {
    if input
        .data
        .as_deref()
        .is_some_and(|value| value.len() > MAX_INLINE_DATA_CHARS)
    {
        return Err(error_result(
            "audio_transcription_too_large",
            "Inline audio data exceeds the 24 MiB broker limit; use path or url.",
        ));
    }
    if input.data.is_some()
        && input
            .file_name
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(error_result(
            "audio_transcription_invalid_arguments",
            "fileName is required when data is provided.",
        ));
    }
    if input.data.is_some()
        && !input
            .data
            .as_deref()
            .unwrap_or_default()
            .starts_with("data:")
        && input
            .mime_type
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(error_result(
            "audio_transcription_invalid_arguments",
            "mimeType is required for raw Base64 data.",
        ));
    }
    Ok(())
}

fn validate_path(path: Option<&str>) -> Result<(), Value> {
    if let Some(path) = path {
        validate_relative_path(path)?;
    }
    Ok(())
}

fn validate_url(url: Option<&str>) -> Result<(), Value> {
    let Some(url) = url else {
        return Ok(());
    };
    let valid = reqwest::Url::parse(url.trim())
        .ok()
        .is_some_and(|url| url.scheme() == "https" && url.host_str().is_some());
    if valid {
        Ok(())
    } else {
        Err(error_result(
            "audio_transcription_invalid_url",
            "Audio URLs must use HTTPS and include a host.",
        ))
    }
}

fn validate_file_name(file_name: Option<&str>) -> Result<(), Value> {
    if let Some(file_name) = file_name {
        if file_name.trim().is_empty()
            || file_name.len() > 255
            || matches!(file_name.trim(), "." | "..")
            || file_name.contains(['/', '\\', '\0', '\r', '\n'])
        {
            return Err(error_result(
                "audio_transcription_invalid_arguments",
                "fileName is invalid.",
            ));
        }
    }
    Ok(())
}

impl TranscribeAudioArguments {
    pub fn source(&self) -> AudioTranscriptionSource {
        AudioTranscriptionSource {
            path: self.path.clone(),
            url: self.url.clone(),
            data: self.data.clone(),
            file_name: self.file_name.clone(),
            mime_type: self.mime_type.clone(),
        }
    }
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
