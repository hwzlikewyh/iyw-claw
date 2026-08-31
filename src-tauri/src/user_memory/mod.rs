mod append;
#[cfg(feature = "memory-bench")]
pub mod bench;
mod candidate_api_types;
mod candidate_lifecycle;
mod candidate_references;
mod candidate_resolution;
mod candidate_store;
mod candidate_types;
mod capabilities;
mod capability_types;
mod context;
mod correction;
mod fs;
mod harvest;
mod harvest_legacy;
mod harvest_store;
mod harvest_store_sql;
mod helpers;
mod index;
mod index_checkpoint;
mod index_fts;
mod index_integrity;
mod index_parse;
mod index_source;
mod index_types;
mod index_verification;
mod journal;
mod launch_context;
mod migration;
mod platform;
mod recall;
mod recall_config;
mod recall_conflict;
mod recall_execute;
mod recall_execute_record;
#[cfg(test)]
mod recall_fallback_scan;
#[cfg(test)]
mod recall_fallback_tests;
mod recall_fts;
mod recall_hydrate;
mod recall_query;
mod recall_query_fts;
mod recall_rank;
mod recall_result;
mod recall_scope;
mod recall_shadow;
mod recall_status;
mod recall_temporal;
mod recall_types;
mod recall_validity;
mod recovery;
mod service;
mod settings_projection;
mod store;
mod structured_file;
mod task_history_store;
mod transaction;
mod types;

pub use candidate_api_types::*;
pub use candidate_types::*;
pub use capabilities::*;
pub use capability_types::*;
pub use context::{
    memory_policy_digest, strip_user_context, MEMORY_POLICY_DOCUMENT, MEMORY_POLICY_REFERENCE,
    MEMORY_POLICY_REVISION, MEMORY_POLICY_SUMMARY, USER_CONTEXT_END, USER_CONTEXT_START,
};
pub use harvest::{
    extract_agent_lessons, harvest_reference, strip_agent_lessons, MemoryHarvestRequest,
    UserMemoryCandidateIndexRebuildResult, UserMemoryHarvestRescanPreview,
    UserMemoryHarvestRescanResult, UserMemoryHarvestState, UserMemoryHarvestStatus,
    UserMemoryHarvestSubmitResult, USER_MEMORY_HARVEST_FILE, USER_MEMORY_HARVEST_SCHEMA_VERSION,
};
pub use recall_scope::UserMemoryRecallScope;
pub use recall_types::{
    UserMemoryIndexStatus, UserMemoryRecallItem, UserMemoryRecallRequest, UserMemoryRecallResult,
    UserMemoryRecallState, USER_MEMORY_MAX_RECALL_LIMIT, USER_MEMORY_MAX_RECALL_QUERY_CHARS,
};
pub use service::UserMemoryService;
pub use transaction::{
    ResourceGeneration, TransactionPhase, UserMemoryGeneration, UserMemoryTransactionJournal,
    USER_MEMORY_TRANSACTION_SCHEMA_VERSION,
};
pub use types::*;

pub(crate) use settings_projection::project_settings_capabilities;

pub(crate) fn prepare_candidate_state_for_restore(
    root: &std::path::Path,
) -> Result<(), crate::app_error::AppCommandError> {
    if candidate_store::read_optional(root)?.is_none() {
        candidate_store::write_state(root, &UserMemoryLearningState::default())?;
    }
    Ok(())
}

pub(crate) fn lock_for_restore_apply(
    root: &std::path::Path,
) -> Result<Option<std::fs::File>, crate::app_error::AppCommandError> {
    let guard = fs::acquire_file_lock(root)?;
    if journal::read(root)?.is_some() {
        return Ok(None);
    }
    Ok(Some(guard))
}
