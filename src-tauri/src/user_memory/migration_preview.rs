use serde::Serialize;
use std::collections::BTreeMap;

use super::{candidate_store, UserMemoryDocumentId, UserMemoryService};
use crate::app_error::AppCommandError;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMigrationPreview {
    pub source_revision: String,
    pub generated_at: String,
    pub counts: BTreeMap<String, usize>,
    pub records: Vec<MemoryMigrationRecord>,
    pub warnings: Vec<String>,
    pub ready_for_shadow_import: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMigrationRecord {
    pub id: String,
    pub kind: String,
    pub content: Option<String>,
    pub content_digest: String,
    pub source_revision: String,
    pub state: String,
    pub scope_type: String,
    pub scope_key: String,
    pub valid_to: Option<String>,
    pub sources: Vec<String>,
}

impl UserMemoryService {
    pub async fn preview_memory_migration(
        &self,
    ) -> Result<MemoryMigrationPreview, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let root = self.resolved_root()?;
        if super::journal::read(root)?.is_some() {
            return Err(super::helpers::conflict(
                "Memory transaction is pending; finish recovery before preview",
            ));
        }
        let policy = self.load_policy_unrecovered().await?;
        let mut settings = super::index_source::readonly_snapshot(self, &policy)?;
        let state = self.read_learning_state()?;
        let revision = super::helpers::hash_parts(&[
            settings.revision.as_bytes(),
            candidate_store::revision(&state)?.as_bytes(),
        ]);
        settings.enabled = true;
        for document in settings.documents.values_mut() {
            document.enabled = true;
        }
        let snapshot = self.scope_index_snapshot(super::index_parse::build_index_snapshot(
            &settings,
            Some(&state),
        ));
        let mut records = snapshot
            .items
            .iter()
            .map(project_record)
            .collect::<Vec<_>>();
        append_missing_records(&mut records, &state);
        let warnings = preview_warnings(&settings, &snapshot, &records);
        let ready_for_shadow_import = warnings
            .iter()
            .all(|warning| warning == "sensitive_content_redacted");
        let mut counts = BTreeMap::new();
        for record in &records {
            *counts.entry(record.kind.clone()).or_default() += 1;
        }
        Ok(MemoryMigrationPreview {
            source_revision: revision,
            generated_at: chrono::Utc::now().to_rfc3339(),
            counts,
            records,
            warnings,
            ready_for_shadow_import,
        })
    }
}

fn project_record(item: &super::index_types::IndexItem) -> MemoryMigrationRecord {
    let active = super::recall_validity::item_is_current_at(item, &chrono::Utc::now());
    MemoryMigrationRecord {
        id: item.id.clone(),
        kind: item.kind.clone(),
        content: (!item.sensitive).then(|| item.content.clone()),
        content_digest: item.content_digest.clone(),
        source_revision: item.source_revision.clone(),
        state: if active { "active" } else { "expired" }.into(),
        scope_type: item.scope_type.clone(),
        scope_key: item.scope_key.clone(),
        valid_to: item.valid_to.clone(),
        sources: item
            .evidence
            .iter()
            .map(|source| source.source_id.clone())
            .collect(),
    }
}

fn append_missing_records(
    records: &mut Vec<MemoryMigrationRecord>,
    state: &super::UserMemoryLearningState,
) {
    for candidate in &state.candidates {
        records.retain(|record| record.id != candidate.id);
        records.push(MemoryMigrationRecord {
            id: candidate.id.clone(),
            kind: "candidate".into(),
            content: (!super::helpers::contains_potential_secret(&candidate.content))
                .then(|| candidate.content.clone()),
            content_digest: candidate.deduplication_digest.clone(),
            source_revision: candidate.last_observed_at.clone(),
            state: serde_json::to_value(candidate.status)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default(),
            scope_type: "global".into(),
            scope_key: String::new(),
            valid_to: None,
            sources: candidate
                .observations
                .iter()
                .map(|source| source.opaque_source_id.clone())
                .collect(),
        });
    }
    append_missing_experiences(records, state);
    append_missing_views(records, state);
}

fn append_missing_views(
    records: &mut Vec<MemoryMigrationRecord>,
    state: &super::UserMemoryLearningState,
) {
    for view in &state.generated_views {
        let id = super::retention_view::document_entry_id(view.document, &view.content);
        if records.iter().any(|record| record.id == id) {
            continue;
        }
        records.push(MemoryMigrationRecord {
            id,
            kind: "generated_view".into(),
            content: Some(view.content.clone()),
            content_digest: super::helpers::hash_parts(&[view.content.as_bytes()]),
            source_revision: String::new(),
            state: "source_invalid_or_overridden".into(),
            scope_type: "global".into(),
            scope_key: String::new(),
            valid_to: None,
            sources: view
                .sources
                .iter()
                .map(|source| source.id.clone())
                .collect(),
        });
    }
}

fn append_missing_experiences(
    records: &mut Vec<MemoryMigrationRecord>,
    state: &super::UserMemoryLearningState,
) {
    for experience in &state.experiences {
        if records.iter().any(|record| record.id == experience.id) {
            continue;
        }
        records.push(MemoryMigrationRecord {
            id: experience.id.clone(),
            kind: "experience".into(),
            content: (!super::helpers::contains_potential_secret(&experience.content))
                .then(|| experience.content.clone()),
            content_digest: experience.content_digest.clone(),
            source_revision: experience.last_observed_at.clone(),
            state: "superseded".into(),
            scope_type: experience.scope_type.clone(),
            scope_key: experience.scope_key.clone(),
            valid_to: state
                .retention
                .get(&experience.id)
                .map(|record| record.expires_at.clone()),
            sources: experience
                .evidence
                .iter()
                .map(|source| source.opaque_source_id.clone())
                .collect(),
        });
    }
}

fn preview_warnings(
    settings: &super::UserMemorySettingsSnapshot,
    snapshot: &super::index_types::IndexSnapshot,
    records: &[MemoryMigrationRecord],
) -> Vec<String> {
    let mut warnings = document_warnings(settings);
    if records.iter().any(|record| record.content.is_none()) {
        warnings.push("sensitive_content_redacted".into());
    }
    if super::index_integrity::validate_snapshot_identities(snapshot).is_err() {
        warnings.push("source_identity_conflict".into());
    }
    warnings
}

fn document_warnings(settings: &super::UserMemorySettingsSnapshot) -> Vec<String> {
    let mut warnings = Vec::new();
    for (id, document) in &settings.documents {
        if !document.readable {
            warnings.push(format!("unreadable:{}", id.file_name()));
        }
        if *id == UserMemoryDocumentId::Memory
            && document.content.lines().any(|line| {
                !line.trim().is_empty()
                    && !line.trim().starts_with('#')
                    && super::index_parse::parse_memory_line(line).is_none()
            })
        {
            warnings.push("unparsed_memory_lines".into());
        }
    }
    warnings
}
