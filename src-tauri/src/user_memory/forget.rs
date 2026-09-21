use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

use super::{
    authority_sql as sql, ResourceGeneration, UserMemoryDocumentId, UserMemoryGeneration,
    UserMemoryLearningState, UserMemoryService,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgetUserMemoryRequest {
    pub id: String,
    pub document: UserMemoryDocumentId,
    pub expected_revision: String,
    pub purge_backups: bool,
    pub confirmation: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgetUserMemoryResult {
    pub forgotten: bool,
    pub revision: String,
    pub purged_backup_paths: Vec<String>,
    pub residual_backup_paths: Vec<String>,
}

struct PreparedForget {
    stable_id: String,
    previous_document: ResourceGeneration<String>,
    next_document: String,
    previous_learning: UserMemoryLearningState,
    next_learning: UserMemoryLearningState,
    forgotten_contents: Vec<String>,
}

impl UserMemoryService {
    pub async fn forget_user_memory(
        &self,
        request: ForgetUserMemoryRequest,
    ) -> Result<ForgetUserMemoryResult, AppCommandError> {
        validate_request(&request)?;
        self.semantic.generation.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        // 与后台刷新保持先索引、后事实锁的顺序，防止旧任务重新写回被删除的内容。
        let _semantic = self
            .semantic
            .task
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppCommandError::task_execution_failed("Memory worker unavailable"))?;
        let _refresh = self.index_refresh_lock.clone().lock_owned().await;
        let (_guard, _file) = self.acquire_locks().await?;
        let authority = self.active_authority().ok_or_else(|| {
            AppCommandError::configuration_invalid(
                "Physical forgetting requires active SQLite memory authority",
            )
        })?;
        let prepared = self.prepare_forget(&request, &authority).await?;
        self.clear_forgotten_vector_projection().await?;
        self.commit_forget(request.document, &prepared).await?;
        let backups = super::forget_backups::process(
            self,
            (&request.id, &prepared.forgotten_contents),
            request.purge_backups,
        )
        .await?;
        self.schedule_index_refresh();
        let revision = self.current_catalog_revision().await?;
        tracing::info!(
            backup_purge = request.purge_backups,
            purged_backups = backups.purged.len(),
            residual_backups = backups.residual.len(),
            "[memory-forget] memory physically removed from primary authority"
        );
        Ok(ForgetUserMemoryResult {
            forgotten: true,
            revision,
            purged_backup_paths: backups.purged,
            residual_backup_paths: backups.residual,
        })
    }

    async fn prepare_forget(
        &self,
        request: &ForgetUserMemoryRequest,
        authority: &super::authority_types::AuthoritySnapshot,
    ) -> Result<PreparedForget, AppCommandError> {
        let policy = self.load_policy_unrecovered().await?;
        let settings = self.snapshot_locked(&policy)?;
        let learning = self.read_learning_state()?;
        let revision = super::entry_catalog::catalog_revision(&settings.revision, &learning)?;
        if revision != request.expected_revision {
            return Err(super::helpers::conflict(
                "Memory changed; refresh before forgetting",
            ));
        }
        let snapshot = super::entry_catalog::management_snapshot(&settings, &learning);
        let item = snapshot
            .items
            .iter()
            .find(|item| item.id == request.id && item.kind == document_kind(request.document))
            .ok_or_else(|| AppCommandError::not_found("Memory entry not found"))?;
        let stable_id =
            super::authority_records::identity(&self.db, &self.authority_key()?, &request.id)
                .await?
                .unwrap_or_else(|| request.id.clone());
        prepare_sources(
            request,
            authority,
            learning,
            item.content.clone(),
            stable_id,
        )
    }

    async fn commit_forget(
        &self,
        document: UserMemoryDocumentId,
        prepared: &PreparedForget,
    ) -> Result<(), AppCommandError> {
        let previous = generation(document, prepared, false)?;
        let next = generation(document, prepared, true)?;
        super::transaction::validate_participation(&previous, &next)?;
        let current = self
            .active_authority()
            .ok_or_else(|| super::helpers::conflict("Memory authority is not active"))?;
        super::authority::validate_change(&current.data, &previous)?;
        let data = super::authority::apply_change(current.data.clone(), &next);
        super::authority_validation::validate(&data)?;
        let snapshot = super::authority_types::AuthoritySnapshot {
            digest: super::authority_types::digest(&data)?,
            epoch: current.epoch + 1,
            data,
            ..current
        };
        let tombstones = prepared
            .forgotten_contents
            .iter()
            .map(|content| forgotten_hash(content))
            .collect::<Vec<_>>();
        self.save_authority_forget_transaction(&snapshot, &prepared.stable_id, &tombstones)
            .await?;
        *self
            .authority
            .write()
            .unwrap_or_else(|error| error.into_inner()) = Some(snapshot.clone());
        if let Err(error) = self.export_authority_locked(&snapshot).await {
            tracing::warn!(code=?error.code,"[memory-forget] compatibility export deferred");
        }
        Ok(())
    }

    pub(super) async fn is_forgotten_content(
        &self,
        content: &str,
    ) -> Result<bool, AppCommandError> {
        let rows = sql::rows(&self.db,"SELECT 1 AS found FROM memory_forget_tombstone WHERE root_key=? AND source_hash=? LIMIT 1",
            vec![self.authority_key()?.into(),forgotten_hash(content).into()]).await?;
        Ok(!rows.is_empty())
    }

    async fn current_catalog_revision(&self) -> Result<String, AppCommandError> {
        let policy = self.load_policy_unrecovered().await?;
        let settings = self.snapshot_locked(&policy)?;
        let learning = self.read_learning_state()?;
        super::entry_catalog::catalog_revision(&settings.revision, &learning)
    }
}

fn prepare_sources(
    request: &ForgetUserMemoryRequest,
    authority: &super::authority_types::AuthoritySnapshot,
    previous_learning: UserMemoryLearningState,
    content: String,
    stable_id: String,
) -> Result<PreparedForget, AppCommandError> {
    let previous_document = authority.data.documents[&request.document].clone();
    let current = match &previous_document {
        ResourceGeneration::Present { value, .. } => value.as_str(),
        ResourceGeneration::Absent => "",
    };
    let next_document = remove_document_entry(request.document, current, &request.id);
    let mut next_learning = previous_learning.clone();
    let mut forgotten_contents = vec![content];
    remove_learning_sources(&mut next_learning, &request.id, &mut forgotten_contents);
    if next_document == current && next_learning == previous_learning {
        return Err(AppCommandError::not_found(
            "Memory source could not be removed",
        ));
    }
    Ok(PreparedForget {
        stable_id,
        previous_document,
        next_document,
        previous_learning,
        next_learning,
        forgotten_contents,
    })
}

fn remove_document_entry(document: UserMemoryDocumentId, current: &str, id: &str) -> String {
    if document == UserMemoryDocumentId::Memory {
        return current
            .lines()
            .filter(|line| {
                super::index_parse::parse_memory_line(line)
                    .is_none_or(|(entry_id, _, _)| entry_id != id)
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    current
        .split("\n\n")
        .filter(|paragraph| {
            super::retention_view::document_entry_id(document, paragraph.trim()) != id
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn remove_learning_sources(
    state: &mut UserMemoryLearningState,
    id: &str,
    forgotten: &mut Vec<String>,
) {
    state.retention.remove(id);
    state.candidates.retain(|candidate| {
        let remove =
            candidate.id == id || candidate.confirmed_memory_entry_id.as_deref() == Some(id);
        if remove {
            forgotten.push(candidate.content.clone());
            forgotten.extend(
                candidate
                    .observations
                    .iter()
                    .filter_map(|item| item.source_excerpt.clone()),
            );
        }
        !remove
    });
    state.experiences.retain(|item| {
        if item.id == id {
            forgotten.push(item.content.clone());
            false
        } else {
            true
        }
    });
    state.generated_views.retain(|view| {
        super::retention_view::document_entry_id(view.document, &view.content) != id
            && !view.sources.iter().any(|source| source.id == id)
    });
    state
        .generated_overrides
        .retain(|item| !item.sources.iter().any(|source| source.id == id));
    if let Some(maintenance) = &mut state.maintenance {
        maintenance.reviews.retain(|review| {
            review.target.id != id && !review.evidence.iter().any(|source| source.id == id)
        });
    }
    state.generated_views_input_digest = None;
}

fn generation(
    document: UserMemoryDocumentId,
    prepared: &PreparedForget,
    next: bool,
) -> Result<UserMemoryGeneration, AppCommandError> {
    Ok(UserMemoryGeneration {
        policy: None,
        documents: BTreeMap::from([(
            document,
            if next {
                super::transaction::document_resource(prepared.next_document.clone())
            } else {
                prepared.previous_document.clone()
            },
        )]),
        candidate_state: Some(super::transaction::candidate_resource(if next {
            prepared.next_learning.clone()
        } else {
            prepared.previous_learning.clone()
        })?),
    })
}

fn validate_request(request: &ForgetUserMemoryRequest) -> Result<(), AppCommandError> {
    if request.confirmation != "FORGET" || request.id.is_empty() || request.id.len() > 128 {
        return Err(AppCommandError::invalid_input(
            "Physical forgetting requires the exact confirmation FORGET",
        ));
    }
    Ok(())
}

fn document_kind(document: UserMemoryDocumentId) -> &'static str {
    match document {
        UserMemoryDocumentId::Memory => "memory",
        UserMemoryDocumentId::Profile => "profile",
        UserMemoryDocumentId::Soul => "soul",
    }
}

fn forgotten_hash(content: &str) -> String {
    super::helpers::hash_parts(&[b"forgotten-content-v1", content.trim().as_bytes()])
}
