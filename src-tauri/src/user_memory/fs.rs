use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::Path;

use crate::app_error::AppCommandError;

use super::helpers::{reject_symlink, validate_document_content};
use super::platform::{open_lock_no_follow, open_read_no_follow};
use super::structured_file::{self, StructuredReadError};
use super::UserMemoryDocumentId;

pub(super) fn acquire_file_lock(root: &Path) -> Result<File, AppCommandError> {
    ensure_safe_root(root)?;
    let path = root.join(".user-memory.lock");
    reject_symlink(&path)?;
    let file = open_lock_no_follow(&path).map_err(AppCommandError::io)?;
    ensure_regular_file(&file)?;
    file.lock().map_err(AppCommandError::io)?;
    cleanup_stale_temporary_files(root)?;
    Ok(file)
}

pub(super) fn read_document(
    root: &Path,
    id: UserMemoryDocumentId,
) -> Result<String, AppCommandError> {
    ensure_safe_root(root)?;
    let path = root.join(id.file_name());
    reject_symlink(&path)?;
    let mut file = match open_read_no_follow(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_empty_no_follow(&path)?;
            open_read_no_follow(&path).map_err(AppCommandError::io)?
        }
        Err(error) => return Err(AppCommandError::io(error)),
    };
    ensure_regular_file(&file)?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(AppCommandError::io)?;
    validate_document_content(&content)?;
    Ok(content)
}

pub(super) fn read_document_optional(
    root: &Path,
    id: UserMemoryDocumentId,
) -> Result<Option<String>, AppCommandError> {
    ensure_safe_root(root)?;
    let path = root.join(id.file_name());
    reject_symlink(&path)?;
    match structured_file::read_bounded_utf8(&path, super::USER_MEMORY_MAX_DOCUMENT_CHARS) {
        Ok(content) => {
            validate_document_content(&content)?;
            Ok(Some(content))
        }
        Err(StructuredReadError::Missing) => Ok(None),
        Err(StructuredReadError::Invalid(detail)) => Err(AppCommandError::configuration_invalid(
            "User memory document is invalid",
        )
        .with_detail(detail)),
        Err(StructuredReadError::Io(error)) => Err(AppCommandError::io(error)),
    }
}

pub(super) fn ensure_document_writable_optional(
    root: &Path,
    id: UserMemoryDocumentId,
) -> Result<(), AppCommandError> {
    structured_file::ensure_writable_optional(root, id.file_name())
}

pub(super) fn apply_document_generation(
    root: &Path,
    id: UserMemoryDocumentId,
    content: Option<&String>,
) -> Result<(), AppCommandError> {
    match content {
        Some(content) => {
            validate_document_content(content)?;
            structured_file::write_bytes_atomic(root, id.file_name(), content.as_bytes())
        }
        None => structured_file::remove_optional(root, id.file_name()),
    }
}

pub(super) fn is_document_readonly(root: &Path, id: UserMemoryDocumentId) -> bool {
    std::fs::symlink_metadata(root.join(id.file_name()))
        .map(|metadata| metadata.permissions().readonly())
        .unwrap_or(false)
}

pub(super) fn ensure_safe_root(root: &Path) -> Result<(), AppCommandError> {
    reject_symlink(root)?;
    std::fs::create_dir_all(root).map_err(AppCommandError::io)?;
    reject_symlink(root)?;
    let metadata = std::fs::symlink_metadata(root).map_err(AppCommandError::io)?;
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(AppCommandError::invalid_input(
            "User memory root is not a directory",
        ))
    }
}

fn create_empty_no_follow(path: &Path) -> Result<(), AppCommandError> {
    match OpenOptions::new().create_new(true).write(true).open(path) {
        Ok(file) => file.sync_all().map_err(AppCommandError::io),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(AppCommandError::io(error)),
    }
}

fn ensure_regular_file(file: &File) -> Result<(), AppCommandError> {
    if file
        .metadata()
        .map_err(AppCommandError::io)?
        .file_type()
        .is_file()
    {
        Ok(())
    } else {
        Err(AppCommandError::permission_denied(
            "User memory documents must be regular files",
        ))
    }
}

fn cleanup_stale_temporary_files(root: &Path) -> Result<(), AppCommandError> {
    for entry in std::fs::read_dir(root).map_err(AppCommandError::io)? {
        let entry = entry.map_err(AppCommandError::io)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if is_user_memory_temporary_file(name) {
            std::fs::remove_file(entry.path()).map_err(AppCommandError::io)?;
        }
    }
    Ok(())
}

fn is_user_memory_temporary_file(name: &str) -> bool {
    let journal_prefix = format!("{}.", super::journal::PENDING_UPDATE_FILE);
    if let Some(suffix) = name.strip_prefix(&journal_prefix) {
        return is_process_uuid_suffix(suffix);
    }
    let migration_prefix = format!("{}.", super::USER_MEMORY_MIGRATION_RECEIPT_FILE);
    if let Some(suffix) = name.strip_prefix(&migration_prefix) {
        return is_process_uuid_suffix(suffix);
    }
    let candidate_prefix = format!("{}.", super::USER_MEMORY_CANDIDATE_FILE);
    if let Some(suffix) = name.strip_prefix(&candidate_prefix) {
        return is_process_uuid_suffix(suffix);
    }
    UserMemoryDocumentId::ALL.iter().any(|id| {
        let current_prefix = format!("{}.", id.file_name());
        if name
            .strip_prefix(&current_prefix)
            .is_some_and(is_process_uuid_suffix)
        {
            return true;
        }
        ["next", "previous"].iter().any(|label| {
            let prefix = format!(".{}.iyw-claw-{label}-", id.file_name());
            name.strip_prefix(&prefix)
                .is_some_and(is_process_uuid_suffix)
        })
    })
}

fn is_process_uuid_suffix(value: &str) -> bool {
    let Some((process_id, rest)) = value.split_once('.') else {
        return false;
    };
    let Some(uuid) = rest.strip_suffix(".tmp") else {
        return false;
    };
    !process_id.is_empty()
        && process_id.bytes().all(|byte| byte.is_ascii_digit())
        && uuid.len() == 32
        && uuid.bytes().all(|byte| byte.is_ascii_hexdigit())
}
