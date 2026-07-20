#[cfg(feature = "tauri-runtime")]
use std::sync::Arc;

use crate::acp::manager::ConnectionManager;
use crate::app_error::AppCommandError;
use crate::user_memory::{
    UserMemoryService, UserMemorySettingsSnapshot, UserMemoryUpdateRequest, UserMemoryUpdateResult,
};

pub async fn get_user_memory_settings_core(
    service: &UserMemoryService,
    manager: &ConnectionManager,
) -> Result<UserMemorySettingsSnapshot, AppCommandError> {
    let mut settings = service.snapshot().await?;
    settings.stale_running_sessions = manager.count_stale_user_memory(service).await;
    Ok(settings)
}

pub async fn update_user_memory_settings_core(
    service: &UserMemoryService,
    manager: &ConnectionManager,
    request: UserMemoryUpdateRequest,
) -> Result<UserMemoryUpdateResult, AppCommandError> {
    let mut settings = service.update(request).await?;
    let affected = manager.count_stale_user_memory(service).await;
    settings.stale_running_sessions = affected;
    Ok(UserMemoryUpdateResult {
        settings,
        affected_running_sessions: affected,
    })
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_user_memory_settings(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    #[cfg(feature = "tauri-runtime")] manager: tauri::State<'_, ConnectionManager>,
) -> Result<UserMemorySettingsSnapshot, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        get_user_memory_settings_core(service.inner().as_ref(), &manager).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn update_user_memory_settings(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    #[cfg(feature = "tauri-runtime")] manager: tauri::State<'_, ConnectionManager>,
    request: UserMemoryUpdateRequest,
) -> Result<UserMemoryUpdateResult, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        update_user_memory_settings_core(service.inner().as_ref(), &manager, request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}
