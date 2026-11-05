//! 微信主动分享的本地交接。平台入口只提交文件，解析和消费由宿主完成。
mod archive;
mod inbox;
mod prepare;
mod transcript;

use chrono::{DateTime, Utc};
pub use inbox::{BatchWriter, Inbox};
pub use prepare::PreparedShare;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_BATCH_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_FILES: usize = 32;
pub const MAX_TEXT_BYTES: u64 = 16 * 1024 * 1024;
pub type Result<T> = std::result::Result<T, ShareError>;

#[derive(Debug, thiserror::Error)]
pub enum ShareError {
    #[error("分享文件读写失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("分享记录格式错误：{0}")]
    Json(#[from] serde_json::Error),
    #[error("聊天归档无法读取：{0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareItem {
    pub name: String,
    pub relative_path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareBatch {
    pub schema_version: u32,
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub items: Vec<ShareItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSummary {
    #[serde(flatten)]
    pub batch: ShareBatch,
    pub imported: bool,
    pub error: Option<String>,
}

pub fn default_root() -> Result<PathBuf> {
    dirs::data_local_dir()
        .map(|root| root.join("app.iywclaw").join("share-inbox"))
        .ok_or_else(|| ShareError::Invalid("无法定位本机分享目录".into()))
}

pub fn invalid(message: &str) -> ShareError {
    ShareError::Invalid(message.to_owned())
}

pub fn safe_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        })
        .take(120)
        .collect();
    let clean = clean.trim().trim_end_matches(['.', ' ']);
    if clean.is_empty() {
        "file".into()
    } else {
        clean.into()
    }
}

pub fn checked_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || !path
            .components()
            .all(|p| matches!(p, std::path::Component::Normal(_)))
    {
        return Err(invalid("分享记录含无效的相对路径"));
    }
    let root = root.canonicalize()?;
    let mut current = root.clone();
    for part in path.components() {
        current.push(part);
        if std::fs::symlink_metadata(&current)?
            .file_type()
            .is_symlink()
        {
            return Err(invalid("分享记录不能引用链接文件"));
        }
    }
    let file = current.canonicalize()?;
    if !file.starts_with(&root) || !file.is_file() {
        return Err(invalid("分享文件不存在或超出批次范围"));
    }
    Ok(file)
}
