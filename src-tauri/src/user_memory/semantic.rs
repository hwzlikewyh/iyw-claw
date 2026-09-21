use std::sync::{Arc, Mutex};

use super::{UserMemoryRecallItem, UserMemoryRecallScope, UserMemoryService};
use crate::app_error::AppCommandError;
use serde::Serialize;

#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
use super::{semantic_index::SemanticIndex, semantic_model};

const PREVIEW_LIMIT: usize = 6;

pub(super) struct SemanticRuntime {
    #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
    pub(super) index: Mutex<Option<SemanticIndex>>,
    pub(super) status: Mutex<SemanticStatus>,
    pub(super) last_activity: Mutex<std::time::Instant>,
    pub(super) task: Arc<tokio::sync::Semaphore>,
    pub(super) refresh_requested: std::sync::atomic::AtomicBool,
}

impl Default for SemanticRuntime {
    fn default() -> Self {
        Self {
            #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
            index: Mutex::new(None),
            status: Mutex::new(SemanticStatus::default()),
            last_activity: Mutex::new(std::time::Instant::now()),
            task: Arc::new(tokio::sync::Semaphore::new(1)),
            refresh_requested: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticStatus {
    pub recall_enabled: bool,
    pub supported: bool,
    pub model_installed: bool,
    pub model_downloading: bool,
    pub retry_pending: bool,
    pub next_retry_at: Option<String>,
    pub ready: bool,
    pub busy: bool,
    pub indexed_items: usize,
    pub last_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticPreview {
    pub items: Vec<UserMemoryRecallItem>,
    pub status: SemanticStatus,
}

impl UserMemoryService {
    pub fn semantic_status(&self) -> SemanticStatus {
        let status = self
            .semantic
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
        {
            let mut status = status;
            status.supported = true;
            status.model_installed = self.resolved_root().is_ok_and(semantic_model::installed);
            status.model_downloading = super::managed_model::downloading();
            status
        }
        #[cfg(not(all(feature = "memory-semantic", target_pointer_width = "64")))]
        {
            status
        }
    }

    pub fn prepare_semantic_index(&self) -> Result<SemanticStatus, AppCommandError> {
        #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
        {
            let permit = self
                .semantic
                .task
                .clone()
                .try_acquire_owned()
                .map_err(|_| super::helpers::conflict("Memory semantic worker is busy"))?;
            let service = self.clone();
            service.touch_semantic_activity();
            service.set_semantic_busy(true);
            tokio::spawn(async move {
                match service.prepare_semantic_inner().await {
                    Ok(Some(count)) => service.finish_semantic(Ok(count)),
                    Ok(None) => service.mark_semantic_released(),
                    Err(error) => service.finish_semantic(Err(error)),
                }
                drop(permit);
                service.release_semantic_if_disabled().await;
                if service
                    .semantic
                    .refresh_requested
                    .load(std::sync::atomic::Ordering::Acquire)
                {
                    service.schedule_semantic_refresh();
                }
            });
            Ok(self.semantic_status())
        }
        #[cfg(not(all(feature = "memory-semantic", target_pointer_width = "64")))]
        Err(AppCommandError::configuration_invalid(
            "当前版本暂不支持本地语义检索",
        ))
    }

    pub async fn preview_semantic_memory(
        &self,
        query: String,
    ) -> Result<SemanticPreview, AppCommandError> {
        let (query, _) = super::UserMemoryRecallRequest {
            query,
            limit: Some(PREVIEW_LIMIT),
        }
        .normalized()?;
        let items = self
            .semantic_items(&query, &UserMemoryRecallScope::global())
            .await?;
        Ok(SemanticPreview {
            items,
            status: self.semantic_settings_status().await?,
        })
    }
}

#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
impl UserMemoryService {
    async fn prepare_semantic_inner(&self) -> Result<Option<usize>, AppCommandError> {
        let channel = crate::update::preferences::load(&self.db)
            .await?
            .channel
            .as_str()
            .to_string();
        self.prepare_managed_model(&crate::system_skills::data_dir_from_env(), &channel)
            .await?;
        if !self.semantic_recall_enabled().await? {
            return Ok(None);
        }
        self.synchronize_semantic().await.map(Some)
    }

    pub(super) fn schedule_semantic_refresh(&self) {
        if !self.semantic_recently_used() || !self.semantic_status().model_installed {
            return;
        }
        let service = self.clone();
        tokio::spawn(async move {
            match service.semantic_recall_enabled().await {
                Ok(true) => service.schedule_enabled_semantic_refresh(),
                Ok(false) => {}
                Err(error) => tracing::warn!(
                    code = ?error.code,
                    "[memory-semantic] recall setting unavailable; refresh skipped"
                ),
            }
        });
    }

    fn schedule_enabled_semantic_refresh(&self) {
        self.semantic
            .refresh_requested
            .store(true, std::sync::atomic::Ordering::Release);
        let Ok(permit) = self.semantic.task.clone().try_acquire_owned() else {
            return;
        };
        let service = self.clone();
        service.set_semantic_busy(true);
        tokio::spawn(async move {
            loop {
                service
                    .semantic
                    .refresh_requested
                    .store(false, std::sync::atomic::Ordering::Release);
                let result = service.synchronize_semantic().await;
                let failed = result.is_err();
                service.finish_semantic(result);
                if failed
                    || !service
                        .semantic
                        .refresh_requested
                        .load(std::sync::atomic::Ordering::Acquire)
                {
                    break;
                }
            }
            drop(permit);
            if service
                .semantic
                .refresh_requested
                .load(std::sync::atomic::Ordering::Acquire)
            {
                service.schedule_index_refresh();
            }
        });
    }

    async fn release_semantic_if_disabled(&self) {
        if matches!(self.semantic_recall_enabled().await, Ok(false)) {
            if let Err(error) = self.release_semantic_runtime().await {
                tracing::warn!(
                    code = ?error.code,
                    "[memory-semantic] disabled runtime release failed"
                );
            }
        }
    }

    async fn synchronize_semantic(&self) -> Result<usize, AppCommandError> {
        let root = self.resolved_root()?.to_path_buf();
        let (snapshot, epoch) = self.read_projection_source().await?;
        let runtime = self.semantic.clone();
        let count = tokio::task::spawn_blocking(move || {
            let mut guard = runtime.index.lock().unwrap_or_else(|e| e.into_inner());
            if guard.is_none() {
                *guard = Some(SemanticIndex::open(&root)?);
            }
            let index = guard.as_mut().expect("initialized index");
            index.synchronize(&snapshot)?;
            Ok::<_, AppCommandError>(index.indexed)
        })
        .await
        .map_err(semantic_model::model_error)??;
        if let Some(epoch) = epoch {
            self.complete_projection(epoch, "vector").await?;
        }
        Ok(count)
    }

    pub(super) fn set_semantic_busy(&self, busy: bool) {
        let mut status = self
            .semantic
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        status.busy = busy;
        if busy {
            status.last_error = None;
        }
    }

    pub(super) fn mark_semantic_released(&self) {
        let mut status = self
            .semantic
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        status.ready = false;
        status.busy = false;
        status.indexed_items = 0;
        status.last_error = None;
    }

    pub(super) fn finish_semantic(&self, result: Result<usize, AppCommandError>) {
        let mut status = self
            .semantic
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        status.busy = false;
        match result {
            Ok(count) => {
                status.ready = true;
                status.indexed_items = count;
                status.last_error = None;
            }
            Err(error) => {
                status.ready = false;
                status.last_error = Some(error.to_string());
                tracing::warn!(code = ?error.code, "[memory-semantic] local index unavailable");
            }
        }
    }
}

#[cfg(not(all(feature = "memory-semantic", target_pointer_width = "64")))]
impl UserMemoryService {
    pub(super) fn schedule_semantic_refresh(&self) {}

    pub(super) async fn release_semantic_runtime(&self) -> Result<(), AppCommandError> {
        Ok(())
    }

    pub(super) async fn release_semantic_if_idle(&self) -> Result<(), AppCommandError> {
        Ok(())
    }
}
