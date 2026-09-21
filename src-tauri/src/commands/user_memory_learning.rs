use crate::app_error::AppCommandError;
#[cfg(feature = "tauri-runtime")]
use crate::user_memory::UserMemoryService;
use crate::user_memory::{BackgroundLearningConfig, BackgroundLearningStatus};
#[cfg(feature = "tauri-runtime")]
use std::sync::Arc;

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_user_memory_learning(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<BackgroundLearningStatus, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.background_learning_status().await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn set_user_memory_learning(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    config: BackgroundLearningConfig,
) -> Result<(), AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.set_background_learning(config).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = config;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn refresh_user_memory_views(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<usize, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.refresh_generated_views().await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}
