use crate::app_error::AppCommandError;
#[cfg(feature = "tauri-runtime")]
use crate::user_memory::UserMemoryService;
use crate::user_memory::{
    MemoryMaintenanceStatus, MemoryMigrationPreview, ReconcileMemoryMigrationRequest,
    ReconcileMemoryMigrationResult, ResolveMemoryReviewRequest,
};
#[cfg(feature = "tauri-runtime")]
use std::sync::Arc;

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_user_memory_maintenance(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<MemoryMaintenanceStatus, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.memory_maintenance_status().await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn reconcile_user_memory_migration(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: ReconcileMemoryMigrationRequest,
) -> Result<ReconcileMemoryMigrationResult, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.reconcile_memory_migration(request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn run_user_memory_maintenance(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<(), AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.run_memory_maintenance().await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn resolve_user_memory_review(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: ResolveMemoryReviewRequest,
) -> Result<(), AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.resolve_memory_review(request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn preview_user_memory_migration(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<MemoryMigrationPreview, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.preview_memory_migration().await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}
