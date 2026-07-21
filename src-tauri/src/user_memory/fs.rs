use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::app_error::AppCommandError;

use super::helpers::{reject_symlink, validate_document_content};
use super::platform::{open_lock_no_follow, open_read_no_follow, replace_file, sync_directory};
use super::UserMemoryDocumentId;

struct PreparedDocumentWrite {
    target: PathBuf,
    staged: PathBuf,
    rollback: PathBuf,
}

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

pub(super) fn is_document_readonly(root: &Path, id: UserMemoryDocumentId) -> bool {
    std::fs::symlink_metadata(root.join(id.file_name()))
        .map(|metadata| metadata.permissions().readonly())
        .unwrap_or(false)
}

pub(super) fn write_documents_atomically(
    root: &Path,
    documents: &[(UserMemoryDocumentId, &str)],
) -> Result<(), AppCommandError> {
    if documents.is_empty() {
        return Ok(());
    }
    ensure_safe_root(root)?;
    let mut prepared = Vec::with_capacity(documents.len());
    for (id, content) in documents {
        validate_document_content(content)?;
        match prepare_document_write(root, *id, content) {
            Ok(write) => prepared.push(write),
            Err(error) => {
                cleanup_prepared_writes(&prepared);
                return Err(error);
            }
        }
    }
    let result = commit_document_writes(root, &mut prepared);
    cleanup_prepared_writes(&prepared);
    result
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

fn prepare_document_write(
    root: &Path,
    id: UserMemoryDocumentId,
    content: &str,
) -> Result<PreparedDocumentWrite, AppCommandError> {
    let target = root.join(id.file_name());
    reject_symlink(&target)?;
    let mut source = open_read_no_follow(&target).map_err(AppCommandError::io)?;
    ensure_regular_file(&source)?;
    let permissions = source
        .metadata()
        .map_err(AppCommandError::io)?
        .permissions();
    if permissions.readonly() {
        return Err(AppCommandError::permission_denied(
            "User memory document is read-only",
        ));
    }
    let mut previous = Vec::new();
    source
        .read_to_end(&mut previous)
        .map_err(AppCommandError::io)?;
    let staged = temporary_path(&target, "next");
    let rollback = temporary_path(&target, "previous");
    write_synced_temp(&staged, content.as_bytes(), &permissions)?;
    if let Err(error) = write_synced_temp(&rollback, &previous, &permissions) {
        let _ = std::fs::remove_file(&staged);
        return Err(error);
    }
    Ok(PreparedDocumentWrite {
        target,
        staged,
        rollback,
    })
}

fn commit_document_writes(
    root: &Path,
    prepared: &mut [PreparedDocumentWrite],
) -> Result<(), AppCommandError> {
    let mut committed = 0;
    for index in 0..prepared.len() {
        if let Err(error) = replace_file(&prepared[index].staged, &prepared[index].target) {
            rollback_document_writes(&prepared[..index]);
            return Err(error);
        }
        committed = index + 1;
    }
    if let Err(error) = sync_directory(root) {
        rollback_document_writes(&prepared[..committed]);
        if let Err(sync_error) = sync_directory(root) {
            tracing::error!("[user-memory] rollback directory sync failed: {sync_error}");
        }
        return Err(error);
    }
    Ok(())
}

fn temporary_path(target: &Path, label: &str) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("memory");
    target.with_file_name(format!(
        ".{name}.iyw-claw-{label}-{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ))
}

fn write_synced_temp(
    path: &Path,
    bytes: &[u8],
    permissions: &std::fs::Permissions,
) -> Result<(), AppCommandError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(AppCommandError::io)?;
    file.write_all(bytes).map_err(AppCommandError::io)?;
    file.flush().map_err(AppCommandError::io)?;
    std::fs::set_permissions(path, permissions.clone()).map_err(AppCommandError::io)?;
    file.sync_all().map_err(AppCommandError::io)
}

fn rollback_document_writes(prepared: &[PreparedDocumentWrite]) {
    for write in prepared.iter().rev() {
        if let Err(error) = replace_file(&write.rollback, &write.target) {
            tracing::error!(
                "[user-memory] failed to roll back {}: {error}",
                write.target.display()
            );
        }
    }
}

fn cleanup_prepared_writes(prepared: &[PreparedDocumentWrite]) {
    for write in prepared {
        let _ = std::fs::remove_file(&write.staged);
        let _ = std::fs::remove_file(&write.rollback);
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
    UserMemoryDocumentId::ALL.iter().any(|id| {
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
