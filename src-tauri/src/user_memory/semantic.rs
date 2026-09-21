use std::sync::{Arc, Mutex};

use super::{UserMemoryRecallItem, UserMemoryRecallScope, UserMemoryService};
use crate::app_error::AppCommandError;
use serde::Serialize;

#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
use super::semantic_index::SemanticIndex;

const PREVIEW_LIMIT: usize = 6;
const REFRESH_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(60);

pub(super) struct SemanticRuntime {
    pub(super) cloud_policy: tokio::sync::Mutex<Option<super::cloud_settings::CloudPolicy>>,
    #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
    pub(super) query_cache: super::semantic_query::QueryCache,
    #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
    pub(super) index: Mutex<Option<SemanticIndex>>,
    pub(super) status: Mutex<SemanticStatus>,
    pub(super) last_activity: Mutex<std::time::Instant>,
    pub(super) task: Arc<tokio::sync::Semaphore>,
    pub(super) refresh_requested: std::sync::atomic::AtomicBool,
    pub(super) refresh_scheduled: std::sync::atomic::AtomicBool,
    pub(super) generation: std::sync::atomic::AtomicU64,
}

impl Default for SemanticRuntime {
    fn default() -> Self {
        Self {
            cloud_policy: tokio::sync::Mutex::new(None),
            #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
            query_cache: super::semantic_query::QueryCache::default(),
            #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
            index: Mutex::new(None),
            status: Mutex::new(SemanticStatus::default()),
            last_activity: Mutex::new(std::time::Instant::now()),
            task: Arc::new(tokio::sync::Semaphore::new(1)),
            refresh_requested: std::sync::atomic::AtomicBool::new(false),
            refresh_scheduled: std::sync::atomic::AtomicBool::new(false),
            generation: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticStatus {
    pub config: super::CloudRetrievalConfig,
    pub recall_enabled: bool,
    pub supported: bool,
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
            self.touch_semantic_activity();
            self.schedule_semantic_refresh();
            Ok(self.semantic_status())
        }
        #[cfg(not(all(feature = "memory-semantic", target_pointer_width = "64")))]
        Err(AppCommandError::configuration_invalid(
            "当前平台暂不支持记忆向量索引",
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
            .recall(
                super::UserMemoryRecallRequest {
                    query,
                    limit: Some(PREVIEW_LIMIT),
                },
                UserMemoryRecallScope::global(),
            )
            .await?
            .items;
        Ok(SemanticPreview {
            items,
            status: self.semantic_settings_status().await?,
        })
    }
}

#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
impl UserMemoryService {
    pub(super) fn schedule_semantic_refresh(&self) {
        use std::sync::atomic::Ordering;
        self.semantic
            .refresh_requested
            .store(true, Ordering::Release);
        if self.semantic.refresh_scheduled.swap(true, Ordering::AcqRel) {
            return;
        }
        let service = self.clone();
        tokio::spawn(async move {
            while service
                .semantic
                .refresh_requested
                .swap(false, Ordering::AcqRel)
            {
                service.wait_for_foreground().await;
                let Ok(permit) = service.semantic.task.clone().acquire_owned().await else {
                    break;
                };
                if !matches!(service.semantic_recall_enabled().await, Ok(true)) {
                    break;
                }
                service.set_semantic_busy(true);
                let result = service.synchronize_semantic().await;
                let failed = result.is_err();
                service.finish_semantic(result);
                drop(permit);
                if failed {
                    service
                        .semantic
                        .refresh_requested
                        .store(true, Ordering::Release);
                    tokio::time::sleep(REFRESH_RETRY_DELAY).await;
                }
            }
            service
                .semantic
                .refresh_scheduled
                .store(false, Ordering::Release);
            if service.semantic.refresh_requested.load(Ordering::Acquire) {
                service.schedule_semantic_refresh();
            }
        });
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
                tracing::warn!(code = ?error.code, "[memory-semantic] cloud index refresh deferred");
            }
        }
    }
}

#[cfg(not(all(feature = "memory-semantic", target_pointer_width = "64")))]
impl UserMemoryService {
    pub(super) fn schedule_semantic_refresh(&self) {}

    pub(super) async fn release_semantic_if_idle(&self) -> Result<(), AppCommandError> {
        Ok(())
    }
}
