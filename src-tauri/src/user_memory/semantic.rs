use std::sync::{Arc, Mutex};

use super::{UserMemoryRecallItem, UserMemoryRecallScope, UserMemoryService};
use crate::app_error::AppCommandError;
use serde::Serialize;

#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
use super::semantic_index::SemanticIndex;

const PREVIEW_LIMIT: usize = 6;
#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
const PREPARE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

pub(super) struct SemanticRuntime {
    pub(super) cloud_policy: tokio::sync::Mutex<Option<super::cloud_settings::CloudPolicy>>,
    #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
    pub(super) query_cache: super::semantic_query::QueryCache,
    #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
    pub(super) index: Mutex<Option<SemanticIndex>>,
    pub(super) status: Mutex<SemanticStatus>,
    pub(super) last_activity: Mutex<std::time::Instant>,
    pub(super) task: Arc<tokio::sync::Semaphore>,
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
            status.busy = self.semantic.task.available_permits() == 0;
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
            self.prepare_semantic_once();
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
        let result = self
            .recall(
                super::UserMemoryRecallRequest {
                    query,
                    limit: Some(PREVIEW_LIMIT),
                },
                UserMemoryRecallScope::global(),
            )
            .await?;
        if result.result_state == super::UserMemoryRecallState::Unavailable {
            tracing::warn!(
                reasons = ?result.reason_codes,
                "[memory-semantic] search preview unavailable"
            );
            return Err(AppCommandError::configuration_invalid(
                "记忆检索暂时不可用，请稍后重试",
            ));
        }
        // 不再为预览状态请求云端目录，保留召回超时后的全文降级结果。
        let mut status = self.semantic_status();
        status.recall_enabled = self.semantic_recall_enabled().await?;
        Ok(SemanticPreview {
            items: result.items,
            status,
        })
    }
}

#[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
impl UserMemoryService {
    fn prepare_semantic_once(&self) {
        let Ok(permit) = self.semantic.task.clone().try_acquire_owned() else {
            return;
        };
        let service = self.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let result = tokio::time::timeout(PREPARE_TIMEOUT, service.synchronize_semantic())
                .await
                .unwrap_or_else(|_| {
                    Err(AppCommandError::network(
                        "Memory index preparation timed out",
                    ))
                });
            service.finish_semantic(result);
        });
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
                tracing::warn!(code = ?error.code, error = %error, "[memory-semantic] on-demand index refresh failed; waiting for next request");
            }
        }
    }
}

#[cfg(not(all(feature = "memory-semantic", target_pointer_width = "64")))]
impl UserMemoryService {
    pub(super) async fn release_semantic_if_idle(&self) -> Result<(), AppCommandError> {
        Ok(())
    }
}
