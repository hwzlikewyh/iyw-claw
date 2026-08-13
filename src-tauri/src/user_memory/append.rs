use std::collections::BTreeMap;

use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

use super::helpers::{
    ensure_agent_write_allowed, ensure_manual_write_allowed, memory_entry_id, normalize_append,
    normalize_candidate, validate_document_content,
};
use super::transaction::{candidate_resource, document_resource};
use super::{candidate_references, candidate_store, structured_file};
use super::{
    AgentMemoryAppend, ResourceGeneration, UserMemoryAppendResult,
    UserMemoryCandidateResolutionResult, UserMemoryDocumentId, UserMemoryGeneration,
    UserMemoryLearningState, UserMemoryService, USER_MEMORY_CANDIDATE_FILE,
};

struct PreparedMemoryAppend {
    appended: bool,
    entry_id: String,
    created_at: String,
    previous_markdown: ResourceGeneration<String>,
    next_markdown: String,
}

enum AppendPolicy {
    Agent,
    AuthorizedSession,
    ManualUser,
}

impl UserMemoryService {
    pub async fn append_agent_memory(
        &self,
        input: AgentMemoryAppend,
    ) -> Result<UserMemoryAppendResult, AppCommandError> {
        self.append_agent_memory_with_policy(input, AppendPolicy::Agent)
            .await
    }

    /// Append for an authenticated connection whose launch token already
    /// captured write permission. Live sessions keep their launch snapshot.
    pub async fn append_agent_memory_authorized(
        &self,
        input: AgentMemoryAppend,
    ) -> Result<UserMemoryAppendResult, AppCommandError> {
        self.append_agent_memory_with_policy(input, AppendPolicy::AuthorizedSession)
            .await
    }

    pub async fn append_user_memory_manual(
        &self,
        input: AgentMemoryAppend,
    ) -> Result<UserMemoryAppendResult, AppCommandError> {
        self.append_agent_memory_with_policy(input, AppendPolicy::ManualUser)
            .await
    }

    async fn append_agent_memory_with_policy(
        &self,
        input: AgentMemoryAppend,
        policy_mode: AppendPolicy,
    ) -> Result<UserMemoryAppendResult, AppCommandError> {
        let content = normalize_append(&input.content)?;
        let (_guard, _file_guard) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
        let policy = self.load_policy_unrecovered().await?;
        match policy_mode {
            AppendPolicy::Agent => ensure_agent_write_allowed(&policy, input.agent_type)?,
            AppendPolicy::ManualUser => ensure_manual_write_allowed(&policy)?,
            AppendPolicy::AuthorizedSession => {}
        }

        let prepared = self.prepare_memory_append(&content, input.agent_type)?;
        let candidate_change = self.reconcile_candidates(&content, &prepared)?;
        if prepared.appended || candidate_change.is_some() {
            self.commit_prepared_append(&prepared, candidate_change.as_ref())
                .await?;
        }
        let revision = self.snapshot_locked(&policy)?.revision;
        Ok(UserMemoryAppendResult {
            appended: prepared.appended,
            entry_id: prepared.entry_id,
            created_at: prepared.created_at,
            revision,
        })
    }

    fn prepare_memory_append(
        &self,
        content: &str,
        agent_type: AgentType,
    ) -> Result<PreparedMemoryAppend, AppCommandError> {
        let previous_markdown = self.read_document_resource(UserMemoryDocumentId::Memory)?;
        let mut next_markdown = match &previous_markdown {
            ResourceGeneration::Absent => String::new(),
            ResourceGeneration::Present { value, .. } => value.clone(),
        };
        let entry_id = memory_entry_id(content);
        let created_at = chrono::Utc::now().to_rfc3339();
        let appended = !next_markdown.contains(&entry_id);
        if appended {
            if !next_markdown.is_empty() && !next_markdown.ends_with('\n') {
                next_markdown.push('\n');
            }
            next_markdown.push_str(&format!(
                "- [{}] [{}] {} <!-- {} -->\n",
                created_at, agent_type, content, entry_id
            ));
            validate_document_content(&next_markdown)?;
        }
        Ok(PreparedMemoryAppend {
            appended,
            entry_id,
            created_at,
            previous_markdown,
            next_markdown,
        })
    }

    fn reconcile_candidates(
        &self,
        content: &str,
        prepared: &PreparedMemoryAppend,
    ) -> Result<Option<(UserMemoryLearningState, UserMemoryLearningState)>, AppCommandError> {
        let root = self.resolved_root()?;
        let previous = match candidate_store::read_optional(root) {
            Ok(Some(state)) => state,
            Ok(None) => return Ok(None),
            Err(error) => {
                tracing::warn!(
                    "[user-memory] candidate reconciliation skipped after confirmed append: {error}"
                );
                return Ok(None);
            }
        };
        let mut next = previous.clone();
        let matched_ids = next
            .candidates
            .iter()
            .filter(|candidate| candidate_matches_entry(candidate, &prepared.entry_id))
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        if matched_ids.is_empty() {
            return Ok(None);
        }
        for candidate_id in &matched_ids {
            let confirmed = candidate_references::ConfirmedMemory {
                content,
                entry_id: &prepared.entry_id,
                resolved_at: &prepared.created_at,
            };
            confirm_candidate_and_references(&mut next, candidate_id, &confirmed);
        }
        if let Err(error) =
            structured_file::ensure_writable_optional(root, USER_MEMORY_CANDIDATE_FILE)
        {
            tracing::warn!(
                "[user-memory] candidate reconciliation skipped after confirmed append: {error}"
            );
            return Ok(None);
        }
        Ok(Some((previous, next)))
    }

    pub(super) async fn confirm_candidate_locked(
        &self,
        mut state: UserMemoryLearningState,
        index: usize,
        edited_content: Option<String>,
    ) -> Result<UserMemoryCandidateResolutionResult, AppCommandError> {
        let content = normalize_candidate(
            edited_content
                .as_deref()
                .unwrap_or(&state.candidates[index].content),
        )?;
        let agent_type = state.candidates[index]
            .observations
            .last()
            .ok_or_else(|| {
                AppCommandError::configuration_invalid("Memory candidate has no source")
            })?
            .agent_type;
        let prepared = self.prepare_memory_append(&content, agent_type)?;
        let previous = state.clone();
        let matched_ids = state
            .candidates
            .iter()
            .enumerate()
            .filter(|(candidate_index, candidate)| {
                *candidate_index == index || candidate_matches_entry(candidate, &prepared.entry_id)
            })
            .map(|(_, candidate)| candidate.id.clone())
            .collect::<Vec<_>>();
        for candidate_id in matched_ids {
            let confirmed = candidate_references::ConfirmedMemory {
                content: &content,
                entry_id: &prepared.entry_id,
                resolved_at: &prepared.created_at,
            };
            confirm_candidate_and_references(&mut state, &candidate_id, &confirmed);
        }
        self.commit_prepared_append(&prepared, Some(&(previous, state.clone())))
            .await?;
        Ok(UserMemoryCandidateResolutionResult {
            candidate: state.candidates[index].clone(),
            revision: candidate_store::revision(&state)?,
        })
    }

    async fn commit_prepared_append(
        &self,
        prepared: &PreparedMemoryAppend,
        candidate_change: Option<&(UserMemoryLearningState, UserMemoryLearningState)>,
    ) -> Result<(), AppCommandError> {
        let previous_candidate = candidate_change
            .map(|(previous, _)| candidate_resource(previous.clone()))
            .transpose()?;
        let next_candidate = candidate_change
            .map(|(_, next)| candidate_resource(next.clone()))
            .transpose()?;
        self.execute_transaction(
            UserMemoryGeneration {
                policy: None,
                documents: BTreeMap::from([(
                    UserMemoryDocumentId::Memory,
                    prepared.previous_markdown.clone(),
                )]),
                candidate_state: previous_candidate,
            },
            UserMemoryGeneration {
                policy: None,
                documents: BTreeMap::from([(
                    UserMemoryDocumentId::Memory,
                    document_resource(prepared.next_markdown.clone()),
                )]),
                candidate_state: next_candidate,
            },
        )
        .await
    }
}

fn candidate_matches_entry(candidate: &super::UserMemoryCandidate, entry_id: &str) -> bool {
    !candidate.status.is_terminal() && memory_entry_id(&candidate.content) == entry_id
}

fn confirm_candidate_and_references(
    state: &mut UserMemoryLearningState,
    candidate_id: &str,
    confirmed: &candidate_references::ConfirmedMemory<'_>,
) {
    let affected =
        candidate_references::confirm_candidate_and_references(state, candidate_id, confirmed);
    if affected > 0 {
        tracing::info!(
            candidate_id,
            affected,
            "[user-memory] confirmed referenced candidates"
        );
    }
}
