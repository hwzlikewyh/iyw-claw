use serde::{Deserialize, Serialize};

use super::{UserMemoryDocumentId, UserMemoryService};
use crate::app_error::AppCommandError;

const BACKUP_DIGEST_CHARS: usize = 12;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMigrationIssue {
    pub line_number: usize,
    pub content: Option<String>,
    pub content_digest: String,
    pub sensitive: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReconcileMemoryMigrationRequest {
    pub expected_revision: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileMemoryMigrationResult {
    pub converted: usize,
    pub backup_file: Option<String>,
}

impl UserMemoryService {
    pub async fn reconcile_memory_migration(
        &self,
        request: ReconcileMemoryMigrationRequest,
    ) -> Result<ReconcileMemoryMigrationResult, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        if self.active_authority().is_some() {
            return Err(AppCommandError::invalid_input(
                "Memory authority is already active",
            ));
        }
        let root = self.resolved_root()?;
        let revision = self.migration_source_revision().await?;
        if revision != request.expected_revision {
            return Err(super::helpers::conflict(
                "Memory changed after migration preview; preview again",
            ));
        }
        let content = super::fs::read_document_optional(root, UserMemoryDocumentId::Memory)?
            .unwrap_or_default();
        let (next, converted) = convert_unparsed_lines(&content);
        if converted == 0 {
            return Ok(ReconcileMemoryMigrationResult {
                converted,
                backup_file: None,
            });
        }
        super::helpers::validate_document_content(&next)?;
        let backup_file = write_backup(root, &content)?;
        super::structured_file::write_bytes_atomic(
            root,
            UserMemoryDocumentId::Memory.file_name(),
            next.as_bytes(),
        )?;
        tracing::info!(
            converted,
            backup_file,
            "[memory-migration] legacy lines reconciled"
        );
        self.schedule_index_refresh();
        Ok(ReconcileMemoryMigrationResult {
            converted,
            backup_file: Some(backup_file),
        })
    }

    async fn migration_source_revision(&self) -> Result<String, AppCommandError> {
        let policy = self.load_policy_unrecovered().await?;
        let settings = super::index_source::readonly_snapshot(self, &policy)?;
        let state = self.read_learning_state()?;
        super::migration_preview::source_revision(&settings.revision, &state)
    }
}

pub(super) fn is_unparsed_memory_line(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty()
        && !line.starts_with('#')
        && super::index_parse::parse_memory_line(line).is_none()
}

pub(super) fn unparsed_memory_lines(content: &str) -> Vec<MemoryMigrationIssue> {
    content
        .lines()
        .enumerate()
        .filter(|(_, line)| is_unparsed_memory_line(line))
        .map(|(index, line)| {
            let value = line.trim();
            let sensitive = super::helpers::contains_potential_secret(value);
            MemoryMigrationIssue {
                line_number: index + 1,
                content: (!sensitive).then(|| value.to_string()),
                content_digest: super::helpers::hash_parts(&[value.as_bytes()]),
                sensitive,
            }
        })
        .collect()
}

fn convert_unparsed_lines(content: &str) -> (String, usize) {
    let mut output = String::with_capacity(content.len());
    let mut converted = 0;
    for chunk in content.split_inclusive('\n') {
        let (line, newline) = chunk
            .strip_suffix('\n')
            .map_or((chunk, ""), |line| (line, "\n"));
        let (visible, carriage) = line
            .strip_suffix('\r')
            .map_or((line, ""), |line| (line, "\r"));
        output.push_str(visible);
        if is_unparsed_memory_line(visible) {
            let value = super::index_parse::strip_memory_prefix(visible);
            let id = super::helpers::memory_entry_id(&value);
            output.push_str(&format!(" <!-- {id} -->"));
            converted += 1;
        }
        output.push_str(carriage);
        output.push_str(newline);
    }
    (output, converted)
}

fn write_backup(root: &std::path::Path, content: &str) -> Result<String, AppCommandError> {
    let digest = super::helpers::hash_parts(&[content.as_bytes()]);
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let nonce = uuid::Uuid::new_v4().simple();
    let file = format!(
        ".user-memory-migration-backup-{stamp}-{}-{nonce}.md",
        &digest[..BACKUP_DIGEST_CHARS]
    );
    super::structured_file::install_new_private(root, &file, content.as_bytes())?;
    let path = root.join(&file);
    let mut permissions = std::fs::metadata(&path)
        .map_err(AppCommandError::io)?
        .permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(path, permissions).map_err(AppCommandError::io)?;
    Ok(file)
}
