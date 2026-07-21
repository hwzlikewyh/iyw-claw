use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

use super::transaction::{transaction_invalid, validate_journal};
use super::{structured_file, UserMemoryDocumentId};
use super::{UserMemoryPolicy, UserMemoryTransactionJournal};

pub(super) const PENDING_UPDATE_FILE: &str = ".user-memory.transaction.json";
const MAX_JOURNAL_CHARS: usize = 40 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LegacyPendingUpdate {
    pub(super) previous_policy: UserMemoryPolicy,
    pub(super) next_policy: UserMemoryPolicy,
    pub(super) previous_documents: BTreeMap<UserMemoryDocumentId, String>,
    pub(super) next_documents: BTreeMap<UserMemoryDocumentId, String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredJournal {
    Current(UserMemoryTransactionJournal),
    Legacy(LegacyPendingUpdate),
}

pub(super) enum PendingJournal {
    Current(UserMemoryTransactionJournal),
    Legacy(LegacyPendingUpdate),
}

pub(super) fn read(root: &Path) -> Result<Option<PendingJournal>, AppCommandError> {
    let stored = structured_file::read_json_optional::<StoredJournal>(
        root,
        PENDING_UPDATE_FILE,
        MAX_JOURNAL_CHARS,
    )
    .map_err(|error| transaction_invalid(error.detail.unwrap_or(error.message)))?;
    match stored {
        Some(StoredJournal::Current(transaction)) => {
            validate_journal(&transaction)?;
            Ok(Some(PendingJournal::Current(transaction)))
        }
        Some(StoredJournal::Legacy(legacy)) => Ok(Some(PendingJournal::Legacy(legacy))),
        None => Ok(None),
    }
}

pub(super) fn write(
    root: &Path,
    transaction: &UserMemoryTransactionJournal,
) -> Result<(), AppCommandError> {
    validate_journal(transaction)?;
    structured_file::ensure_writable_optional(root, PENDING_UPDATE_FILE)?;
    structured_file::write_json_atomic(root, PENDING_UPDATE_FILE, transaction)
}

pub(super) fn remove(root: &Path) -> Result<(), AppCommandError> {
    structured_file::remove_optional(root, PENDING_UPDATE_FILE)
}
