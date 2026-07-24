mod append;
mod candidate_api_types;
mod candidate_lifecycle;
mod candidate_resolution;
mod candidate_store;
mod candidate_types;
mod capabilities;
mod capability_types;
mod context;
mod correction;
mod fs;
mod helpers;
mod journal;
mod launch_context;
mod migration;
mod platform;
mod recovery;
mod service;
mod settings_projection;
mod store;
mod structured_file;
mod transaction;
mod types;

pub use candidate_api_types::*;
pub use candidate_types::*;
pub use capabilities::*;
pub use capability_types::*;
pub use context::{strip_user_context, USER_CONTEXT_END, USER_CONTEXT_START};
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
