use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use super::{
    cloud_retrieval::{CloudGateway, CloudVector},
    UserMemoryService,
};
use crate::app_error::AppCommandError;

const CACHE_ITEMS: usize = 24;
const CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const QUERY_DEADLINE: Duration = Duration::from_secs(3);

pub(super) struct QueryCache {
    values: Mutex<VecDeque<(String, CloudVector, Instant)>>,
    task: Arc<tokio::sync::Semaphore>,
    generation: AtomicU64,
}

impl Default for QueryCache {
    fn default() -> Self {
        Self {
            values: Mutex::new(VecDeque::new()),
            task: Arc::new(tokio::sync::Semaphore::new(1)),
            generation: AtomicU64::new(0),
        }
    }
}

impl QueryCache {
    pub fn clear(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.values
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    fn get(&self, key: &str) -> Option<CloudVector> {
        let mut values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        values.retain(|value| value.2.elapsed() < CACHE_TTL);
        values
            .iter()
            .find(|value| value.0 == key)
            .map(|value| value.1.clone())
    }

    fn put(&self, key: String, vector: CloudVector, generation: u64) {
        let mut values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        if self.generation.load(Ordering::Acquire) != generation {
            return;
        }
        if values.len() >= CACHE_ITEMS {
            values.pop_front();
        }
        values.push_back((key, vector, Instant::now()));
    }
}

impl UserMemoryService {
    pub(super) async fn cloud_query_vector(
        &self,
        gateway: CloudGateway,
        model: String,
        query: String,
    ) -> Result<CloudVector, AppCommandError> {
        let key = super::helpers::hash_parts(&[
            gateway.identity.as_bytes(),
            model.as_bytes(),
            query.as_bytes(),
        ]);
        let cache = &self.semantic.query_cache;
        if let Some(vector) = cache.get(&key) {
            return Ok(vector);
        }
        let permit = cache
            .task
            .clone()
            .try_acquire_owned()
            .map_err(|_| super::helpers::conflict("Memory query worker is busy"))?;
        let generation = cache.generation.load(Ordering::Acquire);
        let runtime = self.semantic.clone();
        // 调用方超时只结束等待；唯一的有界任务完成后缓存向量，供 Agent 后续检索复用。
        tokio::spawn(async move {
            let _permit = permit;
            let vector =
                tokio::time::timeout(QUERY_DEADLINE, embed_query(&gateway, &model, &query))
                    .await
                    .map_err(|_| AppCommandError::network("云端记忆检索超时"))??;
            runtime.query_cache.put(key, vector.clone(), generation);
            Ok(vector)
        })
        .await
        .map_err(|_| AppCommandError::task_execution_failed("Memory query interrupted"))?
    }
}

async fn embed_query(
    gateway: &CloudGateway,
    model: &str,
    query: &str,
) -> Result<CloudVector, AppCommandError> {
    let mut merged: Vec<f32> = Vec::new();
    let mut space = String::new();
    for text in super::semantic_chunks::texts(query) {
        let vector = gateway.embed(model, text).await?;
        if merged.is_empty() {
            merged = vec![0.0; vector.values.len()];
            space.clone_from(&vector.space);
        }
        if merged.len() != vector.values.len() || space != vector.space {
            return Err(super::semantic_model::model_error(
                "Embedding dimensions changed",
            ));
        }
        for (target, value) in merged.iter_mut().zip(vector.values) {
            *target += value;
        }
    }
    Ok(CloudVector {
        values: merged,
        space,
    })
}
