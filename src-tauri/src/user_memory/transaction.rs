use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

use super::helpers::{hash_parts, validate_document_content};
use super::{candidate_store, fs, journal, structured_file};
use super::{
    UserMemoryDocumentId, UserMemoryLearningState, UserMemoryPolicy, UserMemoryService,
    USER_MEMORY_CANDIDATE_FILE,
};

pub const USER_MEMORY_TRANSACTION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionPhase {
    Prepared,
    Committed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceGeneration<T> {
    Absent,
    Present { etag: String, value: T },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserMemoryGeneration {
    pub policy: Option<UserMemoryPolicy>,
    pub documents: BTreeMap<UserMemoryDocumentId, ResourceGeneration<String>>,
    pub candidate_state: Option<ResourceGeneration<UserMemoryLearningState>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserMemoryTransactionJournal {
    pub schema_version: u32,
    pub transaction_id: uuid::Uuid,
    pub phase: TransactionPhase,
    pub previous: UserMemoryGeneration,
    pub next: UserMemoryGeneration,
}

impl UserMemoryService {
    pub(super) async fn execute_transaction(
        &self,
        previous: UserMemoryGeneration,
        next: UserMemoryGeneration,
    ) -> Result<(), AppCommandError> {
        let prepared = UserMemoryTransactionJournal {
            schema_version: USER_MEMORY_TRANSACTION_SCHEMA_VERSION,
            transaction_id: uuid::Uuid::new_v4(),
            phase: TransactionPhase::Prepared,
            previous,
            next,
        };
        validate_journal(&prepared)?;
        self.validate_current(&prepared, CurrentExpectation::Previous)
            .await?;
        self.preflight(&prepared.next)?;
        if journal::read(self.resolved_root()?)?.is_some() {
            return Err(transaction_invalid("another transaction is pending"));
        }
        journal::write(self.resolved_root()?, &prepared)?;
        if let Err(error) = self.apply_generation(&prepared.next).await {
            self.rollback_after_error(&prepared).await;
            return Err(error);
        }
        let mut committed = prepared.clone();
        committed.phase = TransactionPhase::Committed;
        if let Err(error) = journal::write(self.resolved_root()?, &committed) {
            self.rollback_after_error(&prepared).await;
            return Err(error);
        }
        journal::remove(self.resolved_root()?)
    }

    pub(super) async fn recover_transaction(
        &self,
        transaction: &UserMemoryTransactionJournal,
    ) -> Result<(), AppCommandError> {
        validate_journal(transaction)?;
        self.validate_current(transaction, CurrentExpectation::Either)
            .await?;
        let target = match transaction.phase {
            TransactionPhase::Prepared => &transaction.previous,
            TransactionPhase::Committed => &transaction.next,
        };
        self.preflight(target)?;
        self.apply_generation(target).await?;
        journal::remove(self.resolved_root()?)
    }

    async fn rollback_after_error(&self, prepared: &UserMemoryTransactionJournal) {
        if let Err(error) = self.recover_transaction(prepared).await {
            tracing::error!("[user-memory] transaction rollback deferred: {error}");
        }
    }

    async fn validate_current(
        &self,
        transaction: &UserMemoryTransactionJournal,
        expectation: CurrentExpectation,
    ) -> Result<(), AppCommandError> {
        if let Some(previous) = transaction.previous.policy.as_ref() {
            let current = self.load_policy_unrecovered().await?;
            validate_member(
                &current,
                previous,
                transaction.next.policy.as_ref().unwrap(),
                expectation,
            )?;
        }
        for (id, previous) in &transaction.previous.documents {
            let current = current_document(self.resolved_root()?, *id)?;
            validate_member(
                &current,
                previous,
                &transaction.next.documents[id],
                expectation,
            )?;
        }
        if let Some(previous) = transaction.previous.candidate_state.as_ref() {
            let current = current_candidate(self.resolved_root()?)?;
            validate_member(
                &current,
                previous,
                transaction.next.candidate_state.as_ref().unwrap(),
                expectation,
            )?;
        }
        Ok(())
    }

    fn preflight(&self, generation: &UserMemoryGeneration) -> Result<(), AppCommandError> {
        let root = self.resolved_root()?;
        for id in generation.documents.keys() {
            fs::ensure_document_writable_optional(root, *id)?;
        }
        if generation.candidate_state.is_some() {
            structured_file::ensure_writable_optional(root, USER_MEMORY_CANDIDATE_FILE)?;
        }
        Ok(())
    }

    async fn apply_generation(
        &self,
        generation: &UserMemoryGeneration,
    ) -> Result<(), AppCommandError> {
        let root = self.resolved_root()?;
        for (id, resource) in &generation.documents {
            fs::apply_document_generation(root, *id, resource_value(resource))?;
        }
        if let Some(resource) = generation.candidate_state.as_ref() {
            match resource {
                ResourceGeneration::Absent => {
                    structured_file::remove_optional(root, USER_MEMORY_CANDIDATE_FILE)?;
                }
                ResourceGeneration::Present { value, .. } => {
                    candidate_store::write_state(root, value)?;
                }
            }
        }
        if let Some(policy) = generation.policy.as_ref() {
            self.save_policy(policy).await?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum CurrentExpectation {
    Previous,
    Either,
}

fn validate_member<T: PartialEq>(
    current: &T,
    previous: &T,
    next: &T,
    expectation: CurrentExpectation,
) -> Result<(), AppCommandError> {
    let valid =
        current == previous || matches!(expectation, CurrentExpectation::Either) && current == next;
    if valid {
        Ok(())
    } else {
        Err(transaction_invalid(
            "current resource generation mismatches journal",
        ))
    }
}

pub(super) fn validate_journal(
    transaction: &UserMemoryTransactionJournal,
) -> Result<(), AppCommandError> {
    if transaction.schema_version != USER_MEMORY_TRANSACTION_SCHEMA_VERSION
        || transaction.transaction_id.is_nil()
    {
        return Err(transaction_invalid("journal identity is invalid"));
    }
    validate_participation(&transaction.previous, &transaction.next)?;
    validate_generation(&transaction.previous)?;
    validate_generation(&transaction.next)
}

fn validate_participation(
    previous: &UserMemoryGeneration,
    next: &UserMemoryGeneration,
) -> Result<(), AppCommandError> {
    let same_policy = previous.policy.is_some() == next.policy.is_some();
    let same_candidate = previous.candidate_state.is_some() == next.candidate_state.is_some();
    let same_documents = previous.documents.keys().eq(next.documents.keys());
    let has_resource = previous.policy.is_some()
        || previous.candidate_state.is_some()
        || !previous.documents.is_empty();
    if same_policy && same_candidate && same_documents && has_resource {
        Ok(())
    } else {
        Err(transaction_invalid("transaction resource sets are invalid"))
    }
}

fn validate_generation(generation: &UserMemoryGeneration) -> Result<(), AppCommandError> {
    for resource in generation.documents.values() {
        if let ResourceGeneration::Present { etag, value } = resource {
            validate_document_content(value)?;
            if *etag != document_etag(value) {
                return Err(transaction_invalid("document generation etag is invalid"));
            }
        }
    }
    if let Some(ResourceGeneration::Present { etag, value }) = &generation.candidate_state {
        if *etag != candidate_store::revision(value)? {
            return Err(transaction_invalid("candidate generation etag is invalid"));
        }
    }
    Ok(())
}

pub(super) fn document_resource(value: String) -> ResourceGeneration<String> {
    ResourceGeneration::Present {
        etag: document_etag(&value),
        value,
    }
}

pub(super) fn candidate_resource(
    value: UserMemoryLearningState,
) -> Result<ResourceGeneration<UserMemoryLearningState>, AppCommandError> {
    Ok(ResourceGeneration::Present {
        etag: candidate_store::revision(&value)?,
        value,
    })
}

fn document_etag(value: &str) -> String {
    hash_parts(&[value.as_bytes()])
}

fn current_document(
    root: &std::path::Path,
    id: UserMemoryDocumentId,
) -> Result<ResourceGeneration<String>, AppCommandError> {
    Ok(match fs::read_document_optional(root, id)? {
        Some(value) => document_resource(value),
        None => ResourceGeneration::Absent,
    })
}

fn current_candidate(
    root: &std::path::Path,
) -> Result<ResourceGeneration<UserMemoryLearningState>, AppCommandError> {
    match candidate_store::read_optional(root)? {
        Some(value) => candidate_resource(value),
        None => Ok(ResourceGeneration::Absent),
    }
}

fn resource_value<T>(resource: &ResourceGeneration<T>) -> Option<&T> {
    match resource {
        ResourceGeneration::Absent => None,
        ResourceGeneration::Present { value, .. } => Some(value),
    }
}

pub(super) fn transaction_invalid(detail: impl Into<String>) -> AppCommandError {
    AppCommandError::configuration_invalid("User memory transaction is invalid").with_detail(detail)
}
