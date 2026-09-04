use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;

const STDERR_LIMIT: usize = 8 * 1024;
const TOOL_TIMEOUT: Duration = Duration::from_secs(2 * 60);

#[derive(Debug)]
pub(super) enum MediaToolError {
    NotFound(&'static str),
    Spawn(&'static str),
    TimedOut(&'static str),
    Failed {
        tool: &'static str,
        code: Option<i32>,
        stderr: String,
    },
    InvalidOutput(&'static str),
}

impl fmt::Display for MediaToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(tool) => write!(formatter, "{tool} is unavailable"),
            Self::Spawn(tool) => write!(formatter, "{tool} could not be started"),
            Self::TimedOut(tool) => write!(formatter, "{tool} timed out"),
            Self::Failed { tool, code, .. } => {
                write!(formatter, "{tool} failed with exit code {code:?}")
            }
            Self::InvalidOutput(tool) => write!(formatter, "{tool} returned invalid output"),
        }
    }
}

pub(super) struct MediaToolRunner {
    ffmpeg: Option<PathBuf>,
    ffprobe: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ProbeInfo {
    pub(super) duration_seconds: f64,
    pub(super) sample_rate: Option<u32>,
    pub(super) channels: Option<u16>,
    pub(super) bits_per_sample: Option<u16>,
}

#[derive(Deserialize)]
struct ProbeDocument {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: ProbeFormat,
}

#[derive(Deserialize)]
struct ProbeStream {
    #[serde(default)]
    codec_type: String,
    #[serde(default)]
    sample_rate: Option<String>,
    #[serde(default)]
    channels: Option<u16>,
    #[serde(default)]
    bits_per_sample: Option<u16>,
}

#[derive(Deserialize)]
struct ProbeFormat {
    duration: String,
}

impl MediaToolRunner {
    pub(super) fn discover() -> Result<Self, MediaToolError> {
        let ffprobe = resolve_tool("ffprobe", true)?.ok_or(MediaToolError::NotFound("ffprobe"))?;
        let ffmpeg = resolve_tool("ffmpeg", false)?;
        Ok(Self { ffmpeg, ffprobe })
    }

    pub(super) async fn probe(&self, path: &Path) -> Result<ProbeInfo, MediaToolError> {
        let output = self
            .run(
                "ffprobe",
                &[
                    OsString::from("-v"),
                    OsString::from("error"),
                    OsString::from("-select_streams"),
                    OsString::from("a:0"),
                    OsString::from("-show_entries"),
                    OsString::from(
                        "stream=codec_type,sample_rate,channels,bits_per_sample:format=duration",
                    ),
                    OsString::from("-of"),
                    OsString::from("json"),
                    path.as_os_str().to_owned(),
                ],
            )
            .await?;
        let document = serde_json::from_slice::<ProbeDocument>(&output.stdout)
            .map_err(|_| MediaToolError::InvalidOutput("ffprobe"))?;
        let stream = document
            .streams
            .into_iter()
            .find(|stream| stream.codec_type == "audio")
            .ok_or(MediaToolError::InvalidOutput("ffprobe"))?;
        let duration_seconds = document
            .format
            .duration
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or(MediaToolError::InvalidOutput("ffprobe"))?;
        Ok(ProbeInfo {
            duration_seconds,
            sample_rate: stream.sample_rate.and_then(|value| value.parse().ok()),
            channels: stream.channels,
            bits_per_sample: stream.bits_per_sample,
        })
    }

    pub(super) async fn normalize_to_wav(
        &self,
        input: &Path,
        output: &Path,
    ) -> Result<(), MediaToolError> {
        let args = [
            OsString::from("-hide_banner"),
            OsString::from("-loglevel"),
            OsString::from("error"),
            OsString::from("-nostdin"),
            OsString::from("-y"),
            OsString::from("-i"),
            input.as_os_str().to_owned(),
            OsString::from("-map"),
            OsString::from("0:a:0"),
            OsString::from("-vn"),
            OsString::from("-sn"),
            OsString::from("-dn"),
            OsString::from("-ac"),
            OsString::from("1"),
            OsString::from("-ar"),
            OsString::from("16000"),
            OsString::from("-c:a"),
            OsString::from("pcm_s16le"),
            OsString::from("-f"),
            OsString::from("wav"),
            output.as_os_str().to_owned(),
        ];
        self.run("ffmpeg", &args).await.map(|_| ())
    }

    async fn run(
        &self,
        tool: &'static str,
        args: &[OsString],
    ) -> Result<ToolOutput, MediaToolError> {
        let executable = match tool {
            "ffmpeg" => self.ffmpeg.as_ref().ok_or(MediaToolError::NotFound(tool))?,
            "ffprobe" => &self.ffprobe,
            _ => return Err(MediaToolError::NotFound(tool)),
        };
        let output = tokio::time::timeout(
            TOOL_TIMEOUT,
            crate::process::tokio_command(executable)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| MediaToolError::TimedOut(tool))?
        .map_err(|_| MediaToolError::Spawn(tool))?;
        let stderr = truncate(&output.stderr);
        if !output.status.success() {
            return Err(MediaToolError::Failed {
                tool,
                code: output.status.code(),
                stderr,
            });
        }
        Ok(ToolOutput {
            stdout: output.stdout,
        })
    }
}

struct ToolOutput {
    stdout: Vec<u8>,
}

fn resolve_tool(name: &'static str, required: bool) -> Result<Option<PathBuf>, MediaToolError> {
    let env_name = match name {
        "ffmpeg" => "IYW_CLAW_FFMPEG_PATH",
        "ffprobe" => "IYW_CLAW_FFPROBE_PATH",
        _ => return Err(MediaToolError::NotFound("media tool")),
    };
    if let Some(path) = non_empty_env_path(env_name) {
        return Ok(path.is_file().then_some(path));
    }
    if let Some(directory) = non_empty_env_path("IYW_CLAW_MEDIA_BIN_DIR") {
        let path = directory.join(tool_file_name(name));
        if path.is_file() {
            return Ok(Some(path));
        }
    }
    for directory in bundled_directories() {
        let path = directory.join(tool_file_name(name));
        if path.is_file() {
            return Ok(Some(path));
        }
    }
    if let Ok(path) = which::which(name) {
        tracing::warn!(
            tool = name,
            "[AudioTranscription] using media tool from PATH"
        );
        return Ok(Some(path));
    }
    if required {
        Err(MediaToolError::NotFound(name))
    } else {
        Ok(None)
    }
}

fn non_empty_env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn bundled_directories() -> Vec<PathBuf> {
    let Some(executable) = std::env::current_exe().ok() else {
        return Vec::new();
    };
    let Some(parent) = executable.parent() else {
        return Vec::new();
    };
    vec![
        parent.join("resources").join("media"),
        parent.join("..").join("Resources").join("media"),
        parent.join("..").join("resources").join("media"),
    ]
}

fn tool_file_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn truncate(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.chars().take(STDERR_LIMIT).collect()
}
