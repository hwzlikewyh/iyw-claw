use crate::app_error::AppCommandError;
use crate::user_memory::MemoryRecallReceipt;
use crate::user_memory::{
    ActivateMemoryAuthorityRequest, MemoryAuthorityStatus, MemoryEffectivenessStatus,
    MemoryRevisionEntry, RecordMemoryRecallFeedbackRequest,
};
#[cfg(feature = "tauri-runtime")]
use {crate::user_memory::UserMemoryService, std::sync::Arc};

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_user_memory_authority(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<MemoryAuthorityStatus, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.memory_authority_status().await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn prepare_user_memory_authority(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<MemoryAuthorityStatus, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.prepare_memory_authority().await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn activate_user_memory_authority(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: ActivateMemoryAuthorityRequest,
) -> Result<MemoryAuthorityStatus, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.activate_memory_authority(request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_user_memory_history(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    id: String,
) -> Result<Vec<MemoryRevisionEntry>, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.memory_revision_history(id).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = id;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_user_memory_receipts(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    id: String,
) -> Result<Vec<MemoryRecallReceipt>, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.memory_recall_receipts(id).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = id;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn record_memory_recall_feedback(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: RecordMemoryRecallFeedbackRequest,
) -> Result<(), AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.record_memory_recall_feedback(request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_memory_effectiveness(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<MemoryEffectivenessStatus, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.memory_effectiveness_status().await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}
