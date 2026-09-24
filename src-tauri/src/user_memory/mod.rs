mod append;
mod authority;
mod authority_backup;
mod authority_commit;
mod authority_export;
mod authority_history;
mod authority_inventory;
mod authority_migration;
mod authority_receipts;
mod authority_records;
mod authority_recovery;
mod authority_sources;
mod authority_sql;
mod authority_types;
mod authority_validation;
mod background_learning;
mod background_learning_gateway;
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
mod clear;
mod clear_sources;
mod context;
mod context_remote;
mod correction;
mod entry_catalog;
mod forget;
mod forget_backups;
mod forget_projection;
mod foreground;
mod fs;
mod generated_overrides;
mod generated_projection;
mod generated_views;
mod governance;
mod governance_apply;
mod harvest;
mod harvest_legacy;
mod harvest_pending;
mod harvest_reconcile;
mod harvest_rescan;
mod harvest_store;
mod harvest_store_sql;
mod helpers;
mod index;
mod index_candidates;
mod index_checkpoint;
mod index_fts;
mod index_integrity;
mod index_parse;
mod index_source;
mod index_types;
mod index_verification;
mod journal;
mod launch_context;
mod learning;
mod maintenance;
mod maintenance_queue;
mod maintenance_resolution;
mod maintenance_review;
mod maintenance_types;
mod migration;
mod migration_preview;
mod migration_reconcile;
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
mod recall_feedback;
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
mod reconcile;
mod reconcile_files;
mod reconcile_types;
mod recovery;
mod restore_apply;
mod restore_bundle;
mod restore_integrity;
mod restore_sources;
mod retention;
mod retention_view;
mod semantic;
mod cloud_retrieval;
mod cloud_settings;
#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
mod semantic_chunks;
#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
mod semantic_cloud_index;
#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
mod semantic_query;
#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
mod semantic_lifecycle;
#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
mod semantic_storage;
#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
mod semantic_index;
#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
mod semantic_model;
mod semantic_recall;
mod semantic_rerank;
mod semantic_settings;
mod service;
mod settings_projection;
mod store;
mod structured_file;
mod task_history_store;
mod transaction;
mod types;
mod workspace_scope;

pub(crate) use authority_export::MARKER_FILE as USER_MEMORY_AUTHORITY_FILE;
pub use authority_receipts::{MemoryRecallReceipt, MemoryRecallVersion};
pub use authority_types::{
    ActivateMemoryAuthorityRequest, MemoryAuthorityStatus, MemoryRevisionEntry,
};
pub use background_learning::{BackgroundLearningConfig, BackgroundLearningStatus};
pub use candidate_api_types::*;
pub use candidate_types::*;
pub use capabilities::*;
pub use capability_types::*;
pub use clear::{ClearUserMemoryRequest, ClearUserMemoryResult, ClearUserMemoryScope};
pub use context::{
    memory_policy_digest, strip_user_context, MEMORY_POLICY_DOCUMENT, MEMORY_POLICY_REFERENCE,
    MEMORY_POLICY_REVISION, MEMORY_POLICY_SUMMARY, USER_CONTEXT_END, USER_CONTEXT_START,
};
pub use entry_catalog::{
    UserMemoryEntry, UserMemoryEntryListRequest, UserMemoryEntryPage, UserMemoryEntryStatusRequest,
};
pub use forget::*;
pub use generated_overrides::GeneratedViewOverride;
pub use generated_views::{GeneratedMemoryView, GeneratedViewSource};
pub use governance::*;
pub use harvest::{
    assistant_harvest_reference, extract_agent_lessons, harvest_reference, strip_agent_lessons,
    MemoryHarvestRequest, UserMemoryCandidateIndexRebuildResult, UserMemoryHarvestRescanPreview,
    UserMemoryHarvestRescanResult, UserMemoryHarvestState, UserMemoryHarvestStatus,
    UserMemoryHarvestSubmitResult, USER_MEMORY_HARVEST_FILE, USER_MEMORY_HARVEST_SCHEMA_VERSION,
};
pub use maintenance_types::*;
pub use migration_preview::*;
pub use migration_reconcile::*;
pub use recall_feedback::*;
pub use recall_scope::UserMemoryRecallScope;
pub use recall_types::{
    UserMemoryIndexStatus, UserMemoryRecallItem, UserMemoryRecallRequest, UserMemoryRecallResult,
    UserMemoryRecallState, USER_MEMORY_MAX_RECALL_LIMIT, USER_MEMORY_MAX_RECALL_QUERY_CHARS,
};
pub use reconcile_types::*;
pub use retention::{MemoryRetention, RetireMemoryRequest, RetireMemoryResult};
pub use semantic::{SemanticPreview, SemanticStatus};
pub use cloud_retrieval::RetrievalModels;
pub use cloud_settings::CloudRetrievalConfig;
pub use foreground::MemoryForegroundGuard;
pub use service::UserMemoryService;
pub use task_history_store::ContinuationTaskContext;
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
