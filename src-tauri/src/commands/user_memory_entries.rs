#[cfg(feature = "tauri-runtime")]
use std::sync::Arc;

use crate::app_error::AppCommandError;
use crate::user_memory::{
    ApplyMemoryGovernanceRequest, ApplyMemoryGovernanceResult, ForgetUserMemoryRequest,
    ForgetUserMemoryResult, MemoryGovernancePreview, SemanticPreview, SemanticStatus,
    UserMemoryEntryListRequest, UserMemoryEntryPage, UserMemoryEntryStatusRequest,
    UserMemoryService,
};

pub async fn list_user_memory_entries_core(
    service: &UserMemoryService,
    request: UserMemoryEntryListRequest,
) -> Result<UserMemoryEntryPage, AppCommandError> {
    service.list_memory_entries(request).await
}

pub async fn set_user_memory_entry_status_core(
    service: &UserMemoryService,
    request: UserMemoryEntryStatusRequest,
) -> Result<(), AppCommandError> {
    service.set_memory_entry_status(request).await
}

pub async fn preview_memory_governance_core(
    service: &UserMemoryService,
) -> Result<MemoryGovernancePreview, AppCommandError> {
    service.preview_memory_governance().await
}

pub async fn apply_memory_governance_core(
    service: &UserMemoryService,
    request: ApplyMemoryGovernanceRequest,
) -> Result<ApplyMemoryGovernanceResult, AppCommandError> {
    service.apply_memory_governance(request).await
}

pub async fn forget_user_memory_core(
    service: &UserMemoryService,
    request: ForgetUserMemoryRequest,
) -> Result<ForgetUserMemoryResult, AppCommandError> {
    service.forget_user_memory(request).await
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_user_memory_semantic_status(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<SemanticStatus, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.semantic_settings_status().await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn set_user_memory_semantic_enabled(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    enabled: bool,
) -> Result<(), AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.set_semantic_recall_enabled(enabled).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = enabled;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn prepare_user_memory_semantic(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<SemanticStatus, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.prepare_semantic_index()
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn preview_user_memory_semantic(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    query: String,
) -> Result<SemanticPreview, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        service.preview_semantic_memory(query).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = query;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn list_user_memory_entries(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: UserMemoryEntryListRequest,
) -> Result<UserMemoryEntryPage, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        list_user_memory_entries_core(service.inner().as_ref(), request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn set_user_memory_entry_status(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: UserMemoryEntryStatusRequest,
) -> Result<(), AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        set_user_memory_entry_status_core(service.inner().as_ref(), request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn preview_memory_governance(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
) -> Result<MemoryGovernancePreview, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        preview_memory_governance_core(service.inner().as_ref()).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn apply_memory_governance(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: ApplyMemoryGovernanceRequest,
) -> Result<ApplyMemoryGovernanceResult, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        apply_memory_governance_core(service.inner().as_ref(), request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn forget_user_memory(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: ForgetUserMemoryRequest,
) -> Result<ForgetUserMemoryResult, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        forget_user_memory_core(service.inner().as_ref(), request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}
