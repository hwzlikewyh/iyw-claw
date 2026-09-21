use std::sync::atomic::Ordering;

use super::cloud_retrieval::CloudGateway;
use super::index_types::IndexSnapshot;
use super::semantic_chunks::{self, MemoryChunk};
use super::semantic_index::SemanticIndex;
use super::{CloudRetrievalConfig, UserMemoryService};
use crate::app_error::AppCommandError;

pub(super) struct CloudSearch {
    pub snapshot: IndexSnapshot,
    pub scope: super::UserMemoryRecallScope,
    pub identity: String,
    pub model_identity: String,
    pub generation: u64,
    pub vector: Vec<f32>,
}

impl UserMemoryService {
    pub(super) async fn query_cloud_index(
        &self,
        request: CloudSearch,
    ) -> Result<Vec<(String, String, f32)>, AppCommandError> {
        // 先恢复已有向量，再在本次检索时限内补齐，避免冷加载重复嵌入。
        if let Ok(_permit) = self.semantic.task.clone().try_acquire_owned() {
            if self.semantic.generation.load(Ordering::Acquire) != request.generation {
                return Ok(Vec::new());
            }
            self.ensure_cloud_index(
                (request.model_identity.clone(), request.identity.clone()),
                request.vector.len(),
            )
            .await?;
            let result = self.synchronize_semantic().await;
            self.finish_semantic(result);
        }
        let runtime = self.semantic.clone();
        let root = self.resolved_root()?.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut guard = runtime.index.lock().unwrap_or_else(|e| e.into_inner());
            if runtime.generation.load(Ordering::Acquire) != request.generation {
                return Ok(Vec::new());
            }
            if guard
                .as_ref()
                .is_none_or(|index| index.identity != request.identity)
            {
                let Ok(_permit) = runtime.task.clone().try_acquire_owned() else {
                    return Ok(Vec::new());
                };
                *guard = None;
                let mut index =
                    SemanticIndex::open(&root, &request.identity, request.vector.len())?;
                index.model_identity = request.model_identity;
                *guard = Some(index);
            }
            let Some(index) = guard.as_mut() else {
                return Ok(Vec::new());
            };
            index.query(&request.snapshot, request.vector, &request.scope)
        })
        .await
        .map_err(super::semantic_model::model_error)?
    }

    pub(super) async fn synchronize_semantic(&self) -> Result<usize, AppCommandError> {
        if !self.semantic_recall_enabled().await? {
            return Ok(0);
        }
        let generation = self.semantic.generation.load(Ordering::Acquire);
        let gateway = self.cloud_gateway().await?;
        let config = self.resolve_cloud_model(&gateway).await?;
        if config.embedding_model.is_empty() {
            return Ok(0);
        }
        let identity = index_identity(&gateway, &config);
        let (snapshot, epoch) = self.read_projection_source().await?;
        let chunks = current_chunks(&snapshot);
        let expected = chunks
            .iter()
            .map(|chunk| chunk.memory_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        for chunk in chunks {
            if self.semantic.generation.load(Ordering::Acquire) != generation {
                return Ok(0);
            }
            self.index_cloud_chunk(
                &gateway,
                (&config.embedding_model, &identity, generation),
                chunk,
            )
            .await?;
        }
        let count = self.reconcile_cloud_index(snapshot).await?;
        if count == expected {
            if let Some(epoch) = epoch {
                self.complete_projection(epoch, "vector").await?;
            }
        }
        Ok(count)
    }

    async fn index_cloud_chunk(
        &self,
        gateway: &CloudGateway,
        model: (&str, &str, u64),
        chunk: MemoryChunk,
    ) -> Result<(), AppCommandError> {
        let (model, identity, generation) = model;
        let exists = self
            .semantic
            .index
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|index| index.model_identity == identity && index.contains(&chunk.id));
        if exists {
            return Ok(());
        }
        let vector = gateway.embed(model, &chunk.text).await?;
        let space = space_identity(identity, &vector.space);
        self.ensure_cloud_index((identity.to_string(), space), vector.values.len())
            .await?;
        let runtime = self.semantic.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = runtime.index.lock().unwrap_or_else(|e| e.into_inner());
            // 检索超时后阻塞任务仍可能完成，遗忘后的旧结果不可写回。
            if runtime.generation.load(Ordering::Acquire) != generation {
                return Ok(());
            }
            guard
                .as_mut()
                .ok_or_else(|| super::helpers::conflict("Memory index released"))?
                .insert(&chunk, vector.values)
        })
        .await
        .map_err(super::semantic_model::model_error)?
    }

    pub(super) async fn ensure_cloud_index(
        &self,
        identity: (String, String),
        dimension: usize,
    ) -> Result<(), AppCommandError> {
        let runtime = self.semantic.clone();
        let generation = runtime.generation.load(Ordering::Acquire);
        let root = self.resolved_root()?.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let (model_identity, identity) = identity;
            let mut guard = runtime.index.lock().unwrap_or_else(|e| e.into_inner());
            if runtime.generation.load(Ordering::Acquire) != generation {
                return Err(super::helpers::conflict("Memory index generation changed"));
            }
            if guard
                .as_ref()
                .is_some_and(|index| index.identity == identity && index.dimension == dimension)
            {
                return Ok(());
            }
            let changed = guard.as_ref().is_some_and(|index| {
                index.model_identity == model_identity && index.identity != identity
            });
            *guard = None;
            let mut index = SemanticIndex::open(&root, &identity, dimension)?;
            index.model_identity = model_identity;
            *guard = Some(index);
            if changed {
                return Err(super::semantic_model::model_error(
                    "Embedding space changed; rebuilding index",
                ));
            }
            Ok(())
        })
        .await
        .map_err(super::semantic_model::model_error)?
    }

    async fn reconcile_cloud_index(
        &self,
        snapshot: IndexSnapshot,
    ) -> Result<usize, AppCommandError> {
        let runtime = self.semantic.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = runtime.index.lock().unwrap_or_else(|e| e.into_inner());
            let Some(index) = guard.as_mut() else {
                return Ok(0);
            };
            index.synchronize(&snapshot)?;
            Ok(index.indexed)
        })
        .await
        .map_err(super::semantic_model::model_error)?
    }
}

pub(super) fn index_identity(gateway: &CloudGateway, config: &CloudRetrievalConfig) -> String {
    super::helpers::hash_parts(&[
        gateway.identity.as_bytes(),
        config.embedding_model.as_bytes(),
        semantic_chunks::CHUNK_VERSION.as_bytes(),
    ])
}

pub(super) fn space_identity(model: &str, space: &str) -> String {
    super::helpers::hash_parts(&[model.as_bytes(), space.as_bytes()])
}

fn current_chunks(snapshot: &IndexSnapshot) -> Vec<MemoryChunk> {
    let now = chrono::Utc::now();
    snapshot
        .items
        .iter()
        .filter(|item| !item.sensitive && super::recall_validity::item_is_current_at(item, &now))
        .flat_map(semantic_chunks::chunks)
        .collect()
}
