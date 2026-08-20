use std::path::Path;

use crate::acp::audio_transcription::AudioToolFailure;

use super::{new_temp_path, LoadedAudio};

const FLASH_MAX_DURATION_SECONDS: f64 = 2.0 * 60.0 * 60.0;

pub(super) async fn normalize(
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
    let status = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
        .arg(&loaded.path)
        .args(["-vn", "-acodec", "pcm_s16le", "-f", "wav"])
        .arg(&output)
        .status()
        .await
        .map_err(|_| AudioToolFailure::converter_unavailable())?;
    if !status.success() {
        return Err(AudioToolFailure::conversion_failed());
    }
    Ok(LoadedAudio {
        path: output.to_path_buf(),
        file_name: replace_extension(&loaded.file_name, "wav"),
        content_type: Some("audio/wav"),
        temp_path: Some(output),
    })
}

pub(super) async fn enforce_duration(path: &Path) -> Result<(), AudioToolFailure> {
    let output = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nokey=1:noprint_wrappers=1",
        ])
        .arg(path)
        .output()
        .await;
    let Ok(output) = output else {
        return Ok(());
    };
    if !output.status.success() {
        return Ok(());
    }
    let duration = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .ok();
    if duration.is_some_and(|value| value > FLASH_MAX_DURATION_SECONDS) {
        Err(AudioToolFailure::duration_exceeded())
    } else {
        Ok(())
    }
}

fn replace_extension(file_name: &str, extension: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("audio");
    format!("{stem}.{extension}")
}
