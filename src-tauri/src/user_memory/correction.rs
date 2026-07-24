use std::collections::BTreeMap;

use crate::app_error::AppCommandError;

use super::helpers::{
    conflict, ensure_manual_document_write_allowed, memory_entry_id, normalize_append,
    validate_document_update_content,
};
use super::transaction::document_resource;
use super::{
    fs, CorrectUserMemoryRequest, CorrectUserMemoryResult, ResourceGeneration,
    UserMemoryDocumentId, UserMemoryGeneration, UserMemoryService,
};

impl UserMemoryService {
    pub async fn correct_user_memory(
        &self,
        request: CorrectUserMemoryRequest,
    ) -> Result<CorrectUserMemoryResult, AppCommandError> {
        let old_content = normalize_append(&request.old_content)?;
        let new_content = normalize_append(&request.new_content)?;
        if old_content == new_content {
            return Err(AppCommandError::invalid_input(
                "Corrected memory must differ from the existing memory",
            ));
        }

        let (_guard, _file_guard) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
        let policy = self.load_policy_unrecovered().await?;
        ensure_manual_document_write_allowed(&policy, request.document)?;
        let root = self.resolved_root()?;
        fs::ensure_document_writable_optional(root, request.document)?;

        let previous = self.read_document_resource(request.document)?;
        let (etag, current) = match &previous {
            ResourceGeneration::Present { etag, value } => (etag, value),
            ResourceGeneration::Absent => {
                return Err(AppCommandError::not_found(
                    "User memory document does not contain the requested memory",
                ));
            }
        };
        if etag != &request.expected_etag {
            return Err(conflict(
                "User memory document changed; reload before correcting",
            ));
        }

        let old_entry_id = memory_entry_id(&old_content);
        let new_entry_id = memory_entry_id(&new_content);
        let (needle, replacement) = correction_segments(
            request.document,
            current,
            &old_content,
            &new_content,
            &old_entry_id,
            &new_entry_id,
        )?;
        let occurrence_count = current.match_indices(&needle).count();
        if occurrence_count == 0 {
            return Err(AppCommandError::not_found(
                "The existing memory text was not found in the selected document",
            ));
        }
        if occurrence_count > 1 {
            return Err(conflict(
                "The existing memory text appears more than once; edit the document directly",
            ));
        }

        let next = current.replacen(&needle, &replacement, 1);
        validate_document_update_content(&next)?;
        self.execute_transaction(
            UserMemoryGeneration {
                policy: None,
                documents: BTreeMap::from([(request.document, previous)]),
                candidate_state: None,
            },
            UserMemoryGeneration {
                policy: None,
                documents: BTreeMap::from([(request.document, document_resource(next))]),
                candidate_state: None,
            },
        )
        .await?;

        let revision = self.snapshot_locked(&policy)?.revision;
        Ok(CorrectUserMemoryResult {
            document: request.document,
            old_entry_id,
            new_entry_id,
            revision,
        })
    }
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
