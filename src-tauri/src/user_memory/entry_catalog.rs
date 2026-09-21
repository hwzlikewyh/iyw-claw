use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

use super::helpers::{conflict, ensure_manual_document_write_allowed, hash_parts};
use super::index_types::{IndexItem, IndexSnapshot};
use super::{candidate_store, MemoryRetention, UserMemoryDocumentId, UserMemoryService};

const MAX_ENTRY_PAGE_SIZE: usize = 100;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserMemoryEntryListRequest {
    pub document: UserMemoryDocumentId,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub include_inactive: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryEntry {
    pub id: String,
    pub content: String,
    pub active: bool,
    pub expires_at: Option<String>,
    pub source_revision: String,
    pub sources: Vec<String>,
    pub scope_type: String,
    pub scope_key: String,
    pub generated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryEntryPage {
    pub entries: Vec<UserMemoryEntry>,
    pub total: usize,
    pub revision: String,
    pub document_etag: String,
    pub readonly: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserMemoryEntryStatusRequest {
    pub id: String,
    pub document: UserMemoryDocumentId,
    pub expected_revision: String,
    pub active: bool,
}

impl UserMemoryService {
    pub async fn list_memory_entries(
        &self,
        request: UserMemoryEntryListRequest,
    ) -> Result<UserMemoryEntryPage, AppCommandError> {
        if request.query.chars().count() > super::USER_MEMORY_MAX_RECALL_QUERY_CHARS {
            return Err(AppCommandError::invalid_input(
                "Memory entry query is too long",
            ));
        }
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let policy = self.load_policy().await?;
        let settings = self.snapshot_locked(&policy)?;
        let learning = self.read_learning_state()?;
        let revision = catalog_revision(&settings.revision, &learning)?;
        let document = &settings.documents[&request.document];
        if !document.readable {
            return Err(AppCommandError::configuration_invalid(
                "Memory document is not readable",
            ));
        }
        let snapshot = management_snapshot(&settings, &learning);
        let mut entries = matching_entries(snapshot, &request);
        let total = entries.len();
        entries = entries
            .into_iter()
            .skip(request.offset)
            .take(MAX_ENTRY_PAGE_SIZE)
            .collect();
        Ok(UserMemoryEntryPage {
            entries,
            total,
            revision,
            document_etag: document.etag.clone(),
            readonly: document.readonly,
        })
    }

    pub async fn set_memory_entry_status(
        &self,
        request: UserMemoryEntryStatusRequest,
    ) -> Result<(), AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let policy = self.load_policy().await?;
        ensure_manual_document_write_allowed(&policy, request.document)?;
        let settings = self.snapshot_locked(&policy)?;
        let mut learning = self.read_learning_state()?;
        if catalog_revision(&settings.revision, &learning)? != request.expected_revision {
            return Err(conflict("Memory entries changed; reload before updating"));
        }
        let snapshot = management_snapshot(&settings, &learning);
        let item = snapshot
            .items
            .iter()
            .find(|item| item.id == request.id && item.kind == document_kind(request.document))
            .ok_or_else(|| AppCommandError::not_found("Memory entry not found"))?;
        if request.active {
            learning.retention.remove(&item.id);
        } else {
            learning
                .retention
                .insert(item.id.clone(), manual_retention(item));
        }
        update_generated_override(&mut learning, &item.id, request.active)?;
        self.persist_learning_state(&learning).await?;
        self.schedule_index_refresh();
        tracing::info!(active = request.active, document = ?request.document,
            "[memory-entry] user updated entry validity");
        Ok(())
    }
}

fn update_generated_override(
    state: &mut super::UserMemoryLearningState,
    id: &str,
    active: bool,
) -> Result<(), AppCommandError> {
    let Some(view) = state
        .generated_views
        .iter()
        .find(|view| super::retention_view::document_entry_id(view.document, &view.content) == id)
        .cloned()
    else {
        return Ok(());
    };
    if active {
        super::generated_overrides::unblock_view(state, &view);
        Ok(())
    } else {
        super::generated_overrides::block_view(state, &view)
    }
}

pub(super) fn catalog_revision(
    settings: &str,
    learning: &super::UserMemoryLearningState,
) -> Result<String, AppCommandError> {
    Ok(hash_parts(&[
        settings.as_bytes(),
        candidate_store::revision(learning)?.as_bytes(),
    ]))
}

pub(super) fn management_snapshot(
    settings: &super::UserMemorySettingsSnapshot,
    learning: &super::UserMemoryLearningState,
) -> IndexSnapshot {
    let mut readable = settings.clone();
    readable.enabled = true;
    for document in readable.documents.values_mut() {
        document.enabled = true;
    }
    let mut manageable = learning.clone();
    manageable.generated_overrides.clear();
    let mut snapshot = super::index_parse::build_index_snapshot(&readable, Some(&manageable));
    for view in learning
        .generated_views
        .iter()
        .filter(|view| !super::generated_overrides::permits(learning, view))
    {
        let id = super::retention_view::document_entry_id(view.document, &view.content);
        if let Some(item) = snapshot.items.iter_mut().find(|item| item.id == id) {
            item.valid_to = Some("1970-01-01T00:00:00.000Z".into());
        }
    }
    snapshot
}

fn matching_entries(
    snapshot: IndexSnapshot,
    request: &UserMemoryEntryListRequest,
) -> Vec<UserMemoryEntry> {
    let query = request.query.trim().to_lowercase();
    let now = Utc::now();
    snapshot
        .items
        .into_iter()
        .filter_map(|item| {
            if item.kind != document_kind(request.document) || item.sensitive {
                return None;
            }
            let active = super::recall_validity::item_is_current_at(&item, &now);
            if (!active && !request.include_inactive)
                || !item.content.to_lowercase().contains(&query)
            {
                return None;
            }
            let generated = item
                .evidence
                .iter()
                .any(|evidence| evidence.source_kind == "generated_view");
            Some(UserMemoryEntry {
                id: item.id,
                content: item.content,
                active,
                expires_at: item.valid_to,
                source_revision: item.source_revision,
                sources: item
                    .evidence
                    .into_iter()
                    .map(|evidence| evidence.source_id)
                    .collect(),
                scope_type: item.scope_type,
                scope_key: item.scope_key,
                generated,
            })
        })
        .collect()
}

fn manual_retention(item: &IndexItem) -> MemoryRetention {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    MemoryRetention {
        content_digest: item.content_digest.clone(),
        source_revision: item.source_revision.clone(),
        expires_at: now.clone(),
        reason: "Disabled by the user in memory settings".to_string(),
        updated_at: now,
    }
}

fn document_kind(document: UserMemoryDocumentId) -> &'static str {
    match document {
        UserMemoryDocumentId::Memory => "memory",
        UserMemoryDocumentId::Profile => "profile",
        UserMemoryDocumentId::Soul => "soul",
    }
}
