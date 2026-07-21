#[cfg(feature = "tauri-runtime")]
use std::sync::Arc;

use crate::acp::manager::ConnectionManager;
use crate::app_error::AppCommandError;
use crate::user_memory::{
    UserMemoryCandidateDeleteRequest, UserMemoryCandidateDeleteResult,
    UserMemoryCandidateListRequest, UserMemoryCandidatePage, UserMemoryCandidateResolutionResponse,
    UserMemoryCandidateResolveRequest, UserMemoryCandidateSummary, UserMemoryService,
    UserMemorySettingsSnapshot, UserMemoryUpdateRequest, UserMemoryUpdateResult,
    USER_MEMORY_CANDIDATE_MAX_LIMIT,
};

pub async fn list_user_memory_candidates_core(
    service: &UserMemoryService,
    request: UserMemoryCandidateListRequest,
) -> Result<UserMemoryCandidatePage, AppCommandError> {
    if request.limit == 0 || request.limit > USER_MEMORY_CANDIDATE_MAX_LIMIT {
        return Err(AppCommandError::invalid_input(
            "Candidate list limit must be between 1 and 100",
        ));
    }
    let snapshot = service.list_candidates().await?;
    let filtered = snapshot
        .candidates
        .iter()
        .filter(|candidate| {
            request
                .status
                .is_none_or(|filter| filter.matches(candidate.status))
        })
        .collect::<Vec<_>>();
    let total = filtered.len() as u32;
    let candidates = filtered
        .into_iter()
        .skip(request.offset as usize)
        .take(request.limit as usize)
        .map(UserMemoryCandidateSummary::from)
        .collect();
    Ok(UserMemoryCandidatePage {
        candidates,
        total,
        offset: request.offset,
        limit: request.limit,
        revision: snapshot.revision,
    })
}

pub async fn resolve_user_memory_candidate_core(
    service: &UserMemoryService,
    request: UserMemoryCandidateResolveRequest,
) -> Result<UserMemoryCandidateResolutionResponse, AppCommandError> {
    Ok(service.resolve_candidate(request).await?.into())
}

pub async fn delete_user_memory_candidate_core(
    service: &UserMemoryService,
    request: UserMemoryCandidateDeleteRequest,
) -> Result<UserMemoryCandidateDeleteResult, AppCommandError> {
    service.delete_candidate(request).await
}

pub async fn get_user_memory_settings_core(
    service: &UserMemoryService,
    manager: &ConnectionManager,
) -> Result<UserMemorySettingsSnapshot, AppCommandError> {
    let mut settings = service.settings_snapshot().await?;
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

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn list_user_memory_candidates(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: UserMemoryCandidateListRequest,
) -> Result<UserMemoryCandidatePage, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        list_user_memory_candidates_core(service.inner().as_ref(), request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn resolve_user_memory_candidate(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: UserMemoryCandidateResolveRequest,
) -> Result<UserMemoryCandidateResolutionResponse, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        resolve_user_memory_candidate_core(service.inner().as_ref(), request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn delete_user_memory_candidate(
    #[cfg(feature = "tauri-runtime")] service: tauri::State<'_, Arc<UserMemoryService>>,
    request: UserMemoryCandidateDeleteRequest,
) -> Result<UserMemoryCandidateDeleteResult, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        delete_user_memory_candidate_core(service.inner().as_ref(), request).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = request;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}
