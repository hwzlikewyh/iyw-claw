use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use image::ImageFormat;
use serde::{Serialize, Deserialize};

use crate::app_error::AppCommandError;

mod prepare;
mod storage;
mod resource;
pub(crate) mod formats;
mod source;
mod special;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatImageResourceContext {
    pub agent_type: Option<crate::models::AgentType>,
    pub model_id: Option<String>,
}

#[cfg(feature = "tauri-runtime")]
pub(crate) use prepare::effective_app_data_dir;
pub(crate) use prepare::{prepare_chat_image_core, PrepareChatImageRequest};
pub(crate) use storage::{stage_chat_image_bytes_core, StageChatImageBytes};

pub const CHAT_IMAGE_SOURCE_MAX_BYTES: u64 = 100 * 1024 * 1024;
pub const CHAT_IMAGE_INPUT_MAX_BYTES: usize = 32 * 1024 * 1024;
pub const CHAT_IMAGE_I18N_KEY_TOO_LARGE: &str = "errors.chatImage.tooLarge";
const CHAT_IMAGE_DECODE_MAX_EDGE: u32 = 16_384;
const CHAT_IMAGE_DECODE_MAX_ALLOC: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedChatImage {
    pub url: Option<String>,
    pub data: Option<String>,
    pub local_path: Option<String>,
    pub mime_type: String,
    pub name: String,
    pub source_bytes: u64,
    pub derived_bytes: usize,
    pub width: u32,
    pub height: u32,
}

pub(crate) struct EncodedChatImage {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
    pub name: String,
    pub source_bytes: u64,
    pub width: u32,
    pub height: u32,
}

pub(crate) fn source_too_large_error(file_name: &str, source_bytes: u64) -> AppCommandError {
    let mib = 1024.0 * 1024.0;
    let params = BTreeMap::from([
        ("name".to_string(), file_name.to_string()),
        (
            "size".to_string(),
            format!("{:.1}", source_bytes as f64 / mib),
        ),
        (
            "limit".to_string(),
            (CHAT_IMAGE_SOURCE_MAX_BYTES / (1024 * 1024)).to_string(),
        ),
    ]);
    AppCommandError::invalid_input("Image exceeds the 100 MB source limit")
        .with_i18n(CHAT_IMAGE_I18N_KEY_TOO_LARGE, params)
}

fn supported_mime(format: ImageFormat) -> Result<&'static str, AppCommandError> {
    match format {
        ImageFormat::Png => Ok("image/png"),
        ImageFormat::Jpeg => Ok("image/jpeg"),
        ImageFormat::WebP => Ok("image/webp"),
        ImageFormat::Gif => Ok("image/gif"),
        _ => Err(AppCommandError::invalid_input(
            "Image format is not supported",
        )),
    }
}

fn inspect_image(bytes: &[u8]) -> Result<(ImageFormat, u32, u32), AppCommandError> {
    let reader = source::limited_reader(bytes)?;
    let format = reader
        .format()
        .ok_or_else(|| AppCommandError::invalid_input("Image format is not supported"))?;
    supported_mime(format)?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(formats::raster_error)?;
    if width == 0 || height == 0 || width > CHAT_IMAGE_DECODE_MAX_EDGE || height > CHAT_IMAGE_DECODE_MAX_EDGE {
        return Err(formats::dimension_error());
    }
    Ok((format, width, height))
}

pub(crate) fn inspect_input_image_mime(bytes: &[u8]) -> Result<&'static str, AppCommandError> {
    let (format, _, _) = inspect_image(bytes)?;
    supported_mime(format)
}

fn prepare_sync(
    path: &Path,
    source_bytes: u64,
    name: String,
) -> Result<EncodedChatImage, AppCommandError> {
    let bytes = std::fs::read(path).map_err(|error| {
        AppCommandError::io_error("Unable to read image").with_detail(error.to_string())
    })?;
    if bytes.len() as u64 > CHAT_IMAGE_SOURCE_MAX_BYTES {
        return Err(source_too_large_error(&name, bytes.len() as u64));
    }

    source::prepare(bytes, source_bytes, name)
}

pub(crate) async fn encode_chat_image_path(
    path: PathBuf,
    name: String,
) -> Result<EncodedChatImage, AppCommandError> {
    if !path.is_absolute() {
        return Err(AppCommandError::invalid_input(
            "Image path must be absolute",
        ));
    }
    let metadata = tokio::fs::metadata(&path).await.map_err(|error| {
        AppCommandError::io_error("Unable to inspect image").with_detail(error.to_string())
    })?;
    if !metadata.is_file() {
        return Err(AppCommandError::invalid_input("Image path is not a file"));
    }
    if metadata.len() > CHAT_IMAGE_SOURCE_MAX_BYTES {
        return Err(source_too_large_error(&name, metadata.len()));
    }
    let source_bytes = metadata.len();
    tokio::task::spawn_blocking(move || prepare_sync(&path, source_bytes, name))
        .await
        .map_err(|error| {
            AppCommandError::task_execution_failed("Image processing task failed")
                .with_detail(error.to_string())
        })?
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[cfg(feature = "tauri-runtime")]
pub async fn prepare_chat_image(
    app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::AppDatabase>,
    path: String,
    chat_dir: Option<String>,
    session_id: Option<String>,
    resource_context: Option<ChatImageResourceContext>,
) -> Result<PreparedChatImage, AppCommandError> {
    let monitor = crate::acp::capability_policy::monitor_file_upload(None).await?;
    prepare_chat_image_core(
        &db.conn,
        PrepareChatImageRequest {
            path: PathBuf::from(path),
            data_dir: effective_app_data_dir(&app)?,
            chat_dir: chat_dir.map(PathBuf::from),
            session_id,
            display_name: None,
            resource_context,
        },
        &monitor,
    )
    .await
}
