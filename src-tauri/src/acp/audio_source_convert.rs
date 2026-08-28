use std::path::Path;

use crate::acp::audio_transcription::AudioToolFailure;
use crate::acp::media_tool::{MediaToolError, MediaToolRunner, ProbeInfo};

use super::{new_temp_path, LoadedAudio};

const FLASH_MAX_DURATION_SECONDS: f64 = 2.0 * 60.0 * 60.0;
const STANDARD_MAX_DURATION_SECONDS: f64 = 5.0 * 60.0 * 60.0;

pub(super) async fn normalize(
    tools: &MediaToolRunner,
    loaded: LoadedAudio,
    flash: bool,
) -> Result<LoadedAudio, AudioToolFailure> {
    let supported = loaded
        .content_type
        .is_some_and(|mime| !flash || matches!(mime, "audio/wav" | "audio/mpeg" | "audio/ogg"));
    if supported {
        return Ok(loaded);
    }
    let output = new_temp_path(Some("wav"))?;
    tools
        .normalize_to_wav(&loaded.path, &output)
        .await
        .map_err(map_tool_error)?;
    let probe = tools.probe(&output).await.map_err(map_tool_error)?;
    validate_flash_output(probe)?;
    Ok(LoadedAudio {
        path: output.to_path_buf(),
        file_name: replace_extension(&loaded.file_name, "wav"),
        content_type: Some("audio/wav"),
        temp_path: Some(output),
    })
}

pub(super) async fn enforce_duration(
    tools: &MediaToolRunner,
    path: &Path,
    flash: bool,
) -> Result<(), AudioToolFailure> {
    let probe = tools.probe(path).await.map_err(map_tool_error)?;
    let max_duration = if flash {
        FLASH_MAX_DURATION_SECONDS
    } else {
        STANDARD_MAX_DURATION_SECONDS
    };
    if probe.duration_seconds > max_duration {
        Err(AudioToolFailure::duration_exceeded())
    } else {
        Ok(())
    }
}

pub(super) fn map_tool_error(error: MediaToolError) -> AudioToolFailure {
    match error {
        MediaToolError::NotFound("ffprobe") => AudioToolFailure::probe_unavailable(),
        MediaToolError::NotFound(_) => AudioToolFailure::converter_unavailable(),
        MediaToolError::Spawn("ffprobe") | MediaToolError::InvalidOutput("ffprobe") => {
            AudioToolFailure::probe_failed()
        }
        MediaToolError::Spawn(_) | MediaToolError::InvalidOutput(_) => {
            AudioToolFailure::conversion_failed()
        }
        MediaToolError::TimedOut(tool) => {
            tracing::warn!(tool, "[AudioTranscription] media tool timed out");
            AudioToolFailure::tool_timeout()
        }
        MediaToolError::Failed { tool, code, stderr } => {
            tracing::warn!(
                tool,
                exit_code = ?code,
                stderr_present = !stderr.is_empty(),
                "[AudioTranscription] media tool failed"
            );
            if tool == "ffprobe" {
                AudioToolFailure::probe_failed()
            } else {
                AudioToolFailure::conversion_failed()
            }
        }
    }
}

fn validate_flash_output(probe: ProbeInfo) -> Result<(), AudioToolFailure> {
    if probe.sample_rate != Some(16_000) || probe.channels != Some(1) {
        return Err(AudioToolFailure::conversion_failed());
    }
    if probe.bits_per_sample.is_some_and(|value| value != 16) {
        return Err(AudioToolFailure::conversion_failed());
    }
    Ok(())
}

fn replace_extension(file_name: &str, extension: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("audio");
    format!("{stem}.{extension}")
}
