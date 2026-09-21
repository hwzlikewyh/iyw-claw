use crate::app_error::AppCommandError;
use crate::user_memory::{
    MemoryReconciliation, MemoryReconciliationResult, ResolveMemoryFileRequest,
    RestoreMemoryAuthorityRequest,
};
#[cfg(feature = "tauri-runtime")]
use {crate::user_memory::UserMemoryService, std::sync::Arc};

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_user_memory_reconciliation(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<MemoryReconciliation, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.memory_reconciliation().await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn resolve_user_memory_file(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: ResolveMemoryFileRequest,
) -> Result<MemoryReconciliationResult, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.resolve_memory_file(request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn restore_user_memory_authority(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: RestoreMemoryAuthorityRequest,
) -> Result<MemoryReconciliationResult, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.restore_memory_authority(request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}
