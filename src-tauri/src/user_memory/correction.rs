use std::collections::BTreeMap;

use crate::app_error::AppCommandError;

use super::helpers::{
    conflict, ensure_manual_document_write_allowed, memory_entry_id, normalize_append,
    validate_document_update_content,
};
use super::transaction::{candidate_resource, document_resource};
use super::{
    candidate_references, candidate_store, fs, CorrectUserMemoryRequest, CorrectUserMemoryResult,
    ResourceGeneration, UserMemoryDocumentId, UserMemoryGeneration, UserMemoryLearningState,
    UserMemoryService,
};

struct NormalizedCorrection {
    old_content: String,
    new_content: String,
}

struct PreparedCorrection {
    old_entry_id: String,
    new_entry_id: String,
    previous_document: ResourceGeneration<String>,
    next_document: String,
    candidate_change: Option<(UserMemoryLearningState, UserMemoryLearningState)>,
}

struct CandidateCorrection<'a> {
    document: UserMemoryDocumentId,
    old_entry_id: &'a str,
    confirmed: candidate_references::ConfirmedMemory<'a>,
}

impl UserMemoryService {
    pub async fn correct_user_memory(
        &self,
        request: CorrectUserMemoryRequest,
    ) -> Result<CorrectUserMemoryResult, AppCommandError> {
        let correction = normalize_correction(&request)?;
        let (_guard, _file_guard) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
        let policy = self.load_policy_unrecovered().await?;
        ensure_manual_document_write_allowed(&policy, request.document)?;
        fs::ensure_document_writable_optional(self.resolved_root()?, request.document)?;
        let prepared = self.prepare_correction(&request, &correction)?;
        self.commit_correction(request.document, &prepared).await?;
        let revision = self.snapshot_locked(&policy)?.revision;
        Ok(CorrectUserMemoryResult {
            document: request.document,
            old_entry_id: prepared.old_entry_id,
            new_entry_id: prepared.new_entry_id,
            revision,
        })
    }

    fn prepare_correction(
        &self,
        request: &CorrectUserMemoryRequest,
        correction: &NormalizedCorrection,
    ) -> Result<PreparedCorrection, AppCommandError> {
        let previous_document = self.read_document_resource(request.document)?;
        let current = current_document(&previous_document, &request.expected_etag)?;
        let old_entry_id = memory_entry_id(&correction.old_content);
        let new_entry_id = memory_entry_id(&correction.new_content);
        let next_document = replace_correction(
            request.document,
            current,
            correction,
            &old_entry_id,
            &new_entry_id,
        )?;
        let resolved_at = chrono::Utc::now().to_rfc3339();
        let candidate = CandidateCorrection {
            document: request.document,
            old_entry_id: &old_entry_id,
            confirmed: candidate_references::ConfirmedMemory {
                content: &correction.new_content,
                entry_id: &new_entry_id,
                resolved_at: &resolved_at,
            },
        };
        let candidate_change = self.prepare_candidate_correction(&candidate)?;
        Ok(PreparedCorrection {
            old_entry_id,
            new_entry_id,
            previous_document,
            next_document,
            candidate_change,
        })
    }

    fn prepare_candidate_correction(
        &self,
        correction: &CandidateCorrection<'_>,
    ) -> Result<Option<(UserMemoryLearningState, UserMemoryLearningState)>, AppCommandError> {
        if correction.document != UserMemoryDocumentId::Memory {
            return Ok(None);
        }
        let Some(previous) = candidate_store::read_optional(self.resolved_root()?)? else {
            return Ok(None);
        };
        let mut next = previous.clone();
        let affected = candidate_references::rewrite_memory_entry_references(
            &mut next,
            correction.old_entry_id,
            &correction.confirmed,
        );
        if affected == 0 {
            return Ok(None);
        }
        tracing::info!(
            old_entry_id = correction.old_entry_id,
            new_entry_id = correction.confirmed.entry_id,
            affected,
            "[user-memory] corrected candidate memory entry references"
        );
        Ok(Some((previous, next)))
    }

    async fn commit_correction(
        &self,
        document: UserMemoryDocumentId,
        prepared: &PreparedCorrection,
    ) -> Result<(), AppCommandError> {
        let (previous_candidate, next_candidate) = candidate_generations(prepared)?;
        self.execute_transaction(
            UserMemoryGeneration {
                policy: None,
                documents: BTreeMap::from([(document, prepared.previous_document.clone())]),
                candidate_state: previous_candidate,
            },
            UserMemoryGeneration {
                policy: None,
                documents: BTreeMap::from([(
                    document,
                    document_resource(prepared.next_document.clone()),
                )]),
                candidate_state: next_candidate,
            },
        )
        .await
    }
}

fn normalize_correction(
    request: &CorrectUserMemoryRequest,
) -> Result<NormalizedCorrection, AppCommandError> {
    let correction = NormalizedCorrection {
        old_content: normalize_append(&request.old_content)?,
        new_content: normalize_append(&request.new_content)?,
    };
    if correction.old_content == correction.new_content {
        Err(AppCommandError::invalid_input(
            "Corrected memory must differ from the existing memory",
        ))
    } else {
        Ok(correction)
    }
}

fn current_document<'a>(
    resource: &'a ResourceGeneration<String>,
    expected_etag: &str,
) -> Result<&'a str, AppCommandError> {
    match resource {
        ResourceGeneration::Present { etag, value } if etag == expected_etag => Ok(value),
        ResourceGeneration::Present { .. } => Err(conflict(
            "User memory document changed; reload before correcting",
        )),
        ResourceGeneration::Absent => Err(AppCommandError::not_found(
            "User memory document does not contain the requested memory",
        )),
    }
}

fn replace_correction(
    document: UserMemoryDocumentId,
    current: &str,
    correction: &NormalizedCorrection,
    old_entry_id: &str,
    new_entry_id: &str,
) -> Result<String, AppCommandError> {
    let (needle, replacement) = correction_segments(
        document,
        current,
        &correction.old_content,
        &correction.new_content,
        old_entry_id,
        new_entry_id,
    )?;
    match current.match_indices(&needle).count() {
        0 => Err(AppCommandError::not_found(
            "The existing memory text was not found in the selected document",
        )),
        1 => {
            let next = current.replacen(&needle, &replacement, 1);
            validate_document_update_content(&next)?;
            Ok(next)
        }
        _ => Err(conflict(
            "The existing memory text appears more than once; edit the document directly",
        )),
    }
}

fn candidate_generations(
    prepared: &PreparedCorrection,
) -> Result<
    (
        Option<ResourceGeneration<UserMemoryLearningState>>,
        Option<ResourceGeneration<UserMemoryLearningState>>,
    ),
    AppCommandError,
> {
    let previous = prepared
        .candidate_change
        .as_ref()
        .map(|(previous, _)| candidate_resource(previous.clone()))
        .transpose()?;
    let next = prepared
        .candidate_change
        .as_ref()
        .map(|(_, next)| candidate_resource(next.clone()))
        .transpose()?;
    Ok((previous, next))
}

fn correction_segments(
    document: UserMemoryDocumentId,
    current: &str,
    old_content: &str,
    new_content: &str,
    old_entry_id: &str,
    new_entry_id: &str,
) -> Result<(String, String), AppCommandError> {
    if document != UserMemoryDocumentId::Memory {
        return Ok((old_content.to_string(), new_content.to_string()));
    }

    let new_marker = format!("<!-- {new_entry_id} -->");
    if new_entry_id != old_entry_id && current.contains(&new_marker) {
        return Err(conflict(
            "The corrected memory already exists; edit the document directly",
        ));
    }
    Ok((
        format!("{old_content} <!-- {old_entry_id} -->"),
        format!("{new_content} {new_marker}"),
    ))
}
