use std::path::{Path, PathBuf};

#[cfg(feature = "tauri-runtime")]
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use crate::app_error::AppCommandError;

const MAX_ATTACHMENT_BYTES: usize = 2 * 1024 * 1024;
#[cfg(feature = "tauri-runtime")]
const MAX_ATTACHMENT_BASE64_LEN: usize = MAX_ATTACHMENT_BYTES.div_ceil(3) * 4;
const MAX_ATTACHMENT_NAME_CHARS: usize = 180;
const CONVERSATION_ATTACHMENTS_DIR: &str = "conversation-attachments";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedChatAttachment {
    pub path: String,
}

fn validate_chat_dir_layout(root: &Path, chat_dir: &Path) -> Result<(), AppCommandError> {
    let relative = chat_dir
        .strip_prefix(root)
        .map_err(|_| AppCommandError::invalid_input("Target is not a managed Chat directory"))?;
    let components: Vec<_> = relative.components().collect();
    if components.len() != 2 {
        return Err(AppCommandError::invalid_input(
            "Target is not a managed Chat directory",
        ));
    }
    let date = components[0].as_os_str().to_string_lossy();
    let id = components[1].as_os_str().to_string_lossy();
    chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| AppCommandError::invalid_input("Invalid managed Chat directory date"))?;
    uuid::Uuid::parse_str(&id)
        .map_err(|_| AppCommandError::invalid_input("Invalid managed Chat directory id"))?;
    Ok(())
}

pub(crate) fn is_managed_chat_dir(data_dir: &Path, chat_dir: &Path) -> bool {
    validate_chat_dir_layout(&data_dir.join("chat-sessions"), chat_dir).is_ok()
}

pub(crate) async fn ensure_managed_chat_dir(
    data_dir: &Path,
    chat_dir: &Path,
) -> Result<PathBuf, AppCommandError> {
    let root = data_dir.join("chat-sessions");
    validate_chat_dir_layout(&root, chat_dir)?;
    tokio::fs::create_dir_all(chat_dir)
        .await
        .map_err(AppCommandError::io)?;
    let root = tokio::fs::canonicalize(root)
        .await
        .map_err(AppCommandError::io)?;
    let chat_dir = tokio::fs::canonicalize(chat_dir)
        .await
        .map_err(AppCommandError::io)?;
    validate_chat_dir_layout(&root, &chat_dir)?;
    Ok(chat_dir)
}

fn sanitize_file_name(raw: &str) -> String {
    let base = Path::new(raw)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let cleaned: String = base
        .chars()
        .filter(|ch| !ch.is_control() && !matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        .take(MAX_ATTACHMENT_NAME_CHARS)
        .collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']);
    if cleaned.is_empty() {
        "file".to_string()
    } else {
        cleaned.to_string()
    }
}

fn sanitize_session_bucket(raw: Option<&str>) -> String {
    let cleaned: String = raw
        .unwrap_or("conversation")
        .chars()
        .map(|ch| match ch {
            ch if ch.is_ascii_alphanumeric() => ch,
            '-' | '_' => ch,
            _ => '_',
        })
        .take(80)
        .collect();
    let cleaned = cleaned.trim_matches('_');
    if cleaned.is_empty() {
        "conversation".to_string()
    } else {
        cleaned.to_string()
    }
}

async fn new_attachment_dir(base: &Path) -> Result<PathBuf, AppCommandError> {
    let dir = base.join(uuid::Uuid::new_v4().simple().to_string());
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(AppCommandError::io)?;
    Ok(dir)
}

async fn canonical_source(source_path: &Path) -> Result<PathBuf, AppCommandError> {
    let source = tokio::fs::canonicalize(source_path)
        .await
        .map_err(AppCommandError::io)?;
    if !tokio::fs::metadata(&source)
        .await
        .map_err(AppCommandError::io)?
        .is_file()
    {
        return Err(AppCommandError::invalid_input(
            "Attachment source is not a file",
        ));
    }
    Ok(source)
}

fn user_facing_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = raw.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    raw.to_string()
}

pub async fn stage_chat_attachment_core(
    data_dir: &Path,
    chat_dir: &Path,
    source_path: &Path,
) -> Result<StagedChatAttachment, AppCommandError> {
    let chat_dir = ensure_managed_chat_dir(data_dir, chat_dir).await?;
    let source = canonical_source(source_path).await?;
    let file_name = source
        .file_name()
        .ok_or_else(|| AppCommandError::invalid_input("Attachment file name is missing"))?;
    let attachment_dir = new_attachment_dir(&chat_dir.join("attachments")).await?;
    let destination = attachment_dir.join(file_name);
    if let Err(error) = tokio::fs::copy(&source, &destination).await {
        let _ = tokio::fs::remove_dir_all(&attachment_dir).await;
        return Err(AppCommandError::io(error));
    }
    Ok(StagedChatAttachment {
        path: user_facing_path(&destination),
    })
}

pub async fn stage_chat_attachment_bytes_core(
    data_dir: &Path,
    chat_dir: Option<&Path>,
    session_id: Option<&str>,
    file_name: &str,
    bytes: &[u8],
) -> Result<StagedChatAttachment, AppCommandError> {
    if bytes.is_empty() {
        return Err(AppCommandError::invalid_input("Attachment file is empty"));
    }
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(AppCommandError::invalid_input(
            "Attachment exceeds the size limit",
        ));
    }

    let base = if let Some(chat_dir) = chat_dir {
        ensure_managed_chat_dir(data_dir, chat_dir)
            .await?
            .join("attachments")
    } else {
        data_dir
            .join(CONVERSATION_ATTACHMENTS_DIR)
            .join(sanitize_session_bucket(session_id))
    };
    let attachment_dir = new_attachment_dir(&base).await?;
    let destination = attachment_dir.join(sanitize_file_name(file_name));
    if let Err(error) = tokio::fs::write(&destination, bytes).await {
        let _ = tokio::fs::remove_dir_all(&attachment_dir).await;
        return Err(AppCommandError::io(error));
    }
    Ok(StagedChatAttachment {
        path: user_facing_path(&destination),
    })
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn stage_chat_attachment(
    app: tauri::AppHandle,
    chat_dir: String,
    source_path: String,
) -> Result<StagedChatAttachment, AppCommandError> {
    use tauri::Manager;
    let data_dir = app
        .path()
        .app_data_dir()
        .map(|path| crate::paths::resolve_effective_data_dir(&path))
        .map_err(|error| {
            AppCommandError::io_error("App data directory unavailable")
                .with_detail(error.to_string())
        })?;
    stage_chat_attachment_core(&data_dir, Path::new(&chat_dir), Path::new(&source_path)).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn stage_chat_attachment_bytes(
    app: tauri::AppHandle,
    chat_dir: Option<String>,
    session_id: Option<String>,
    file_name: String,
    data_base64: String,
) -> Result<StagedChatAttachment, AppCommandError> {
    use tauri::Manager;

    if data_base64.len() > MAX_ATTACHMENT_BASE64_LEN {
        return Err(AppCommandError::invalid_input(
            "Attachment exceeds the size limit",
        ));
    }
    let bytes = BASE64.decode(data_base64).map_err(|error| {
        AppCommandError::invalid_input("Attachment payload is not valid base64")
            .with_detail(error.to_string())
    })?;
    let data_dir = app
        .path()
        .app_data_dir()
        .map(|path| crate::paths::resolve_effective_data_dir(&path))
        .map_err(|error| {
            AppCommandError::io_error("App data directory unavailable")
                .with_detail(error.to_string())
        })?;
    stage_chat_attachment_bytes_core(
        &data_dir,
        chat_dir.as_deref().map(Path::new),
        session_id.as_deref(),
        &file_name,
        &bytes,
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;
    use uuid::Uuid;

    use super::{stage_chat_attachment_bytes_core, stage_chat_attachment_core};

    fn chat_dir(data_dir: &std::path::Path) -> PathBuf {
        data_dir
            .join("chat-sessions")
            .join("2026-07-15")
            .join(Uuid::new_v4().simple().to_string())
    }

    #[tokio::test]
    async fn stages_file_inside_managed_chat_directory() {
        let data_dir = tempdir().expect("data dir");
        let chat_dir = chat_dir(data_dir.path());
        std::fs::create_dir_all(&chat_dir).expect("chat dir");
        let source = data_dir.path().join("report.xlsx");
        std::fs::write(&source, b"workbook-bytes").expect("source");

        let staged = stage_chat_attachment_core(data_dir.path(), &chat_dir, &source)
            .await
            .expect("stage attachment");
        let staged_path = PathBuf::from(staged.path);

        assert!(staged_path.starts_with(chat_dir.join("attachments")));
        assert_eq!(
            staged_path.file_name().and_then(|name| name.to_str()),
            Some("report.xlsx")
        );
        assert_eq!(
            std::fs::read(staged_path).expect("staged bytes"),
            b"workbook-bytes"
        );
    }

    #[tokio::test]
    async fn rejects_target_outside_managed_chat_directory() {
        let data_dir = tempdir().expect("data dir");
        let outside = tempdir().expect("outside dir");
        std::fs::create_dir_all(data_dir.path().join("chat-sessions")).expect("managed root");
        let source = data_dir.path().join("report.xlsx");
        std::fs::write(&source, b"workbook-bytes").expect("source");

        let error = stage_chat_attachment_core(data_dir.path(), outside.path(), &source)
            .await
            .expect_err("outside target must be rejected");

        assert!(error.message.contains("managed Chat directory"));
    }

    #[tokio::test]
    async fn stages_bytes_and_recreates_missing_managed_chat_directory() {
        let data_dir = tempdir().expect("data dir");
        let chat_dir = chat_dir(data_dir.path());

        let staged = stage_chat_attachment_bytes_core(
            data_dir.path(),
            Some(&chat_dir),
            Some("tab-1"),
            "report.pdf",
            b"pdf-bytes",
        )
        .await
        .expect("stage bytes");
        let staged_path = PathBuf::from(staged.path);

        assert!(staged_path.starts_with(chat_dir.join("attachments")));
        assert_eq!(std::fs::read(staged_path).unwrap(), b"pdf-bytes");
    }
}
