use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::app_error::AppCommandError;

use super::helpers::validate_document_content;
use super::structured_file::{self, InstallNewResult, StructuredReadError};
use super::{
    UserMemoryDocumentId, UserMemoryMigrationFileResult, UserMemoryMigrationReceipt,
    UserMemoryMigrationReport, UserMemoryMigrationSource, UserMemoryMigrationStatus,
    UserMemoryService, USER_MEMORY_MAX_DOCUMENT_CHARS, USER_MEMORY_MIGRATION_RECEIPT_FILE,
};

const MIGRATION_SCHEMA_VERSION: u32 = 1;
const MAX_RECEIPT_CHARS: usize = 262_144;

struct ValidSource {
    path: PathBuf,
    content: String,
}

#[derive(Default)]
struct SourceScan {
    valid: Vec<ValidSource>,
    invalid: Vec<(PathBuf, String)>,
    io_failed: Vec<(PathBuf, String)>,
}

impl UserMemoryService {
    pub async fn migrate_legacy_documents(
        &self,
        sources: Vec<UserMemoryMigrationSource>,
    ) -> Result<UserMemoryMigrationReport, AppCommandError> {
        let outcome = self.migrate_legacy_documents_inner(sources).await;
        let blocked = match &outcome {
            Ok(report) => retryable_documents(&report.receipt),
            Err(_) => UserMemoryDocumentId::ALL.into_iter().collect(),
        };
        self.set_migration_blocked_documents(blocked);
        outcome
    }

    async fn migrate_legacy_documents_inner(
        &self,
        sources: Vec<UserMemoryMigrationSource>,
    ) -> Result<UserMemoryMigrationReport, AppCommandError> {
        let (_io_guard, _file_guard) = self.acquire_locks().await?;
        let root = self.resolved_root()?.to_path_buf();
        let sources = deduplicate_sources(&root, sources);
        let mut receipt = load_receipt(&root, sources.clone())?;
        for id in UserMemoryDocumentId::ALL {
            if receipt
                .files
                .get(&id)
                .is_some_and(|result| result.status.is_terminal())
            {
                continue;
            }
            receipt
                .files
                .insert(id, migrate_document(&root, id, &sources));
        }
        receipt.considered_sources = sources;
        receipt.updated_at = chrono::Utc::now().to_rfc3339();
        structured_file::write_json_atomic(&root, USER_MEMORY_MIGRATION_RECEIPT_FILE, &receipt)?;
        Ok(UserMemoryMigrationReport {
            warnings: migration_warnings(&receipt),
            receipt,
        })
    }
}

fn retryable_documents(receipt: &UserMemoryMigrationReceipt) -> BTreeSet<UserMemoryDocumentId> {
    receipt
        .files
        .iter()
        .filter_map(|(id, result)| {
            matches!(
                result.status,
                UserMemoryMigrationStatus::SourceIoFailed
                    | UserMemoryMigrationStatus::DestinationIoFailed
            )
            .then_some(*id)
        })
        .collect()
}

fn load_receipt(
    root: &Path,
    sources: Vec<UserMemoryMigrationSource>,
) -> Result<UserMemoryMigrationReceipt, AppCommandError> {
    let receipt = structured_file::read_json_optional::<UserMemoryMigrationReceipt>(
        root,
        USER_MEMORY_MIGRATION_RECEIPT_FILE,
        MAX_RECEIPT_CHARS,
    )?;
    match receipt {
        Some(receipt) if receipt.schema_version == MIGRATION_SCHEMA_VERSION => Ok(receipt),
        Some(_) => Err(AppCommandError::configuration_invalid(
            "User memory migration receipt version is unsupported",
        )),
        None => Ok(UserMemoryMigrationReceipt {
            schema_version: MIGRATION_SCHEMA_VERSION,
            considered_sources: sources,
            files: BTreeMap::new(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }),
    }
}

fn deduplicate_sources(
    root: &Path,
    sources: Vec<UserMemoryMigrationSource>,
) -> Vec<UserMemoryMigrationSource> {
    let root = path_identity(root);
    let mut seen = HashSet::new();
    sources
        .into_iter()
        .filter_map(|mut source| {
            source.path = crate::git_credential::absolutize(&source.path);
            let identity = path_identity(&source.path);
            (identity != root && seen.insert(identity)).then_some(source)
        })
        .collect()
}

fn path_identity(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn migrate_document(
    root: &Path,
    id: UserMemoryDocumentId,
    sources: &[UserMemoryMigrationSource],
) -> UserMemoryMigrationFileResult {
    match std::fs::symlink_metadata(root.join(id.file_name())) {
        Ok(_) => return result(UserMemoryMigrationStatus::SkippedExisting, None, None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return result(
                UserMemoryMigrationStatus::DestinationIoFailed,
                None,
                Some(error.to_string()),
            )
        }
    }
    let scan = scan_sources(id, sources);
    if let Some(chosen) = scan.valid.first() {
        return install_valid_source(root, id, chosen, &scan);
    }
    failed_source_result(scan)
}

fn scan_sources(id: UserMemoryDocumentId, sources: &[UserMemoryMigrationSource]) -> SourceScan {
    let mut scan = SourceScan::default();
    for source in sources {
        let path = source.path.join(id.file_name());
        match structured_file::read_bounded_utf8(&path, USER_MEMORY_MAX_DOCUMENT_CHARS) {
            Ok(content) => match validate_document_content(&content) {
                Ok(()) => scan.valid.push(ValidSource { path, content }),
                Err(error) => scan.invalid.push((path, error.to_string())),
            },
            Err(StructuredReadError::Missing) => {}
            Err(StructuredReadError::Invalid(detail)) => scan.invalid.push((path, detail)),
            Err(StructuredReadError::Io(error)) => scan.io_failed.push((path, error.to_string())),
        }
    }
    scan
}

fn install_valid_source(
    root: &Path,
    id: UserMemoryDocumentId,
    chosen: &ValidSource,
    scan: &SourceScan,
) -> UserMemoryMigrationFileResult {
    let conflicts = scan
        .valid
        .iter()
        .skip(1)
        .filter(|source| source.content != chosen.content)
        .map(|source| source.path.parent().unwrap_or(&source.path).to_path_buf())
        .collect();
    match structured_file::install_new_private(root, id.file_name(), chosen.content.as_bytes()) {
        Ok(InstallNewResult::Installed) => UserMemoryMigrationFileResult {
            status: UserMemoryMigrationStatus::Copied,
            source: chosen.path.parent().map(Path::to_path_buf),
            conflicting_sources: conflicts,
            detail: ignored_source_detail(scan),
        },
        Ok(InstallNewResult::AlreadyExists) => {
            result(UserMemoryMigrationStatus::SkippedExisting, None, None)
        }
        Err(error) => result(
            UserMemoryMigrationStatus::DestinationIoFailed,
            chosen.path.parent().map(Path::to_path_buf),
            Some(error.to_string()),
        ),
    }
}

fn failed_source_result(scan: SourceScan) -> UserMemoryMigrationFileResult {
    if let Some((path, detail)) = scan.io_failed.first() {
        return result(
            UserMemoryMigrationStatus::SourceIoFailed,
            path.parent().map(Path::to_path_buf),
            Some(detail.clone()),
        );
    }
    if let Some((path, detail)) = scan.invalid.first() {
        return result(
            UserMemoryMigrationStatus::InvalidSource,
            path.parent().map(Path::to_path_buf),
            Some(detail.clone()),
        );
    }
    result(UserMemoryMigrationStatus::SourceMissing, None, None)
}

fn ignored_source_detail(scan: &SourceScan) -> Option<String> {
    let count = scan.invalid.len() + scan.io_failed.len();
    (count > 0).then(|| format!("ignored {count} invalid or unreadable source(s)"))
}

fn result(
    status: UserMemoryMigrationStatus,
    source: Option<PathBuf>,
    detail: Option<String>,
) -> UserMemoryMigrationFileResult {
    UserMemoryMigrationFileResult {
        status,
        source,
        conflicting_sources: Vec::new(),
        detail,
    }
}

fn migration_warnings(receipt: &UserMemoryMigrationReceipt) -> Vec<String> {
    let mut warnings = Vec::new();
    for (id, result) in &receipt.files {
        if !result.conflicting_sources.is_empty() {
            warnings.push(format!(
                "conflict detected while migrating {}",
                id.file_name()
            ));
        }
        if matches!(
            result.status,
            UserMemoryMigrationStatus::InvalidSource
                | UserMemoryMigrationStatus::SourceIoFailed
                | UserMemoryMigrationStatus::DestinationIoFailed
        ) {
            warnings.push(format!(
                "migration {:?} for {}",
                result.status,
                id.file_name()
            ));
        }
    }
    warnings
}
