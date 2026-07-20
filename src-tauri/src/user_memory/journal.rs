use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

use super::helpers::reject_symlink;
use super::platform::{open_read_no_follow, replace_file, sync_directory};
use super::{UserMemoryDocumentId, UserMemoryPolicy};

pub(super) const PENDING_UPDATE_FILE: &str = ".user-memory.transaction.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PendingUpdate {
    pub previous_policy: UserMemoryPolicy,
    pub next_policy: UserMemoryPolicy,
    pub previous_documents: BTreeMap<UserMemoryDocumentId, String>,
    pub next_documents: BTreeMap<UserMemoryDocumentId, String>,
}

pub(super) fn read(root: &Path) -> Result<Option<PendingUpdate>, AppCommandError> {
    let path = root.join(PENDING_UPDATE_FILE);
    reject_symlink(&path)?;
    let mut file = match open_read_no_follow(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppCommandError::io(error)),
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(AppCommandError::io)?;
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        AppCommandError::configuration_invalid("User memory transaction journal is invalid")
            .with_detail(error.to_string())
    })
}

pub(super) fn write(root: &Path, pending: &PendingUpdate) -> Result<(), AppCommandError> {
    let target = root.join(PENDING_UPDATE_FILE);
    reject_symlink(&target)?;
    let temp = temporary_path(&target);
    let bytes = serde_json::to_vec(pending)
        .map_err(|error| AppCommandError::configuration_invalid(error.to_string()))?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(AppCommandError::io)?;
        file.write_all(&bytes).map_err(AppCommandError::io)?;
        file.flush().map_err(AppCommandError::io)?;
        file.sync_all().map_err(AppCommandError::io)?;
        replace_file(&temp, &target)?;
        sync_directory(root)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

pub(super) fn remove(root: &Path) -> Result<(), AppCommandError> {
    let path = root.join(PENDING_UPDATE_FILE);
    reject_symlink(&path)?;
    match std::fs::remove_file(&path) {
        Ok(()) => sync_directory(root),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppCommandError::io(error)),
    }
}

fn temporary_path(target: &Path) -> PathBuf {
    target.with_file_name(format!(
        "{PENDING_UPDATE_FILE}.{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ))
}
