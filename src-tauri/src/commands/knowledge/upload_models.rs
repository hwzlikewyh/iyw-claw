use serde::{Deserialize, Serialize};

use super::models;
use crate::app_error::AppCommandError;

pub const CHUNK_BYTES: usize = 512 * 1024;
pub const MIB: u64 = 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = 1024 * MIB;
pub const MAX_UPLOADS: usize = 10;
pub const MAX_STAGED_BYTES: u64 = 2 * MAX_FILE_BYTES;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BeginUpload {
    pub upload_id: String,
    pub scope: String,
    pub category: u8,
    pub folder_id: String,
    pub file_name: String,
    pub mime_type: String,
    pub expected_bytes: u64,
}

impl BeginUpload {
    pub fn validate(&self) -> Result<(), AppCommandError> {
        models::validate_category(self.category)?;
        if uuid::Uuid::parse_str(&self.upload_id).is_err() || !models::valid_id(&self.folder_id) {
            return Err(AppCommandError::invalid_input("上传标识或目录无效"));
        }
        if self.file_name.trim().is_empty()
            || self.file_name.chars().count() > 180
            || self
                .file_name
                .chars()
                .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
        {
            return Err(AppCommandError::invalid_input("文件名无效或过长"));
        }
        if self.mime_type.len() > 128
            || self.mime_type.chars().any(char::is_control)
            || self.mime_type.starts_with("audio/")
        {
            return Err(AppCommandError::invalid_input("文件类型无效"));
        }
        let limit = file_limit(&self.file_name)?;
        if self.expected_bytes == 0 || self.expected_bytes > limit {
            return Err(AppCommandError::invalid_input(format!(
                "文件为空或超过上传限制（{} MB）",
                limit / MIB
            )));
        }
        Ok(())
    }
}

pub fn file_limit(name: &str) -> Result<u64, AppCommandError> {
    let ext = name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let limit = match ext.as_str() {
        "pdf" => MAX_FILE_BYTES,
        "doc" | "docx" | "epub" | "html" | "htm" => 350 * MIB,
        "ppt" | "pptx" => 200 * MIB,
        "md" | "markdown" | "txt" | "vtt" => 10 * MIB,
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "tiff" | "ico" | "dib" | "icns" | "sgi"
        | "jp2" => 8 * MIB,
        "mp4" | "mkv" | "avi" | "mov" | "wmv" => 512 * MIB,
        // 官网未声明这些格式的大小限制；此处仅是客户端传输保护上限。
        "csv" | "xlsx" | "jsonl" | "bitable" => MAX_FILE_BYTES,
        _ => {
            return Err(AppCommandError::invalid_input(
                "暂不支持此文件格式，音频文件暂不可上传",
            ))
        }
    };
    Ok(limit)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UploadRef {
    pub upload_id: String,
    pub scope: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppendUpload {
    pub upload_id: String,
    pub scope: String,
    pub offset: u64,
    pub data_base64: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadPhase {
    Receiving,
    Uploading,
    Registering,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadStatus {
    pub upload_id: String,
    pub phase: UploadPhase,
    pub received_bytes: u64,
    pub sent_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadCredentials {
    pub access_key_id: String,
    pub access_key_secret: String,
    #[serde(default)]
    pub sts_token: String,
    #[serde(default)]
    pub security_token: String,
    pub region: String,
    pub endpoint: String,
    pub bucket: String,
    pub domain: String,
    pub object_prefix: String,
    pub folder_id: String,
}

#[derive(Clone)]
pub struct UploadedObject {
    pub key: String,
    pub public_url: String,
    pub tos_url: String,
}
