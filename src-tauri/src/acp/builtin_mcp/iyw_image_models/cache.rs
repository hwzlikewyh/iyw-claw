use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use rmcp::ErrorData;
use serde_json::Value;
use tokio::sync::{Mutex, OnceCell};

const CATALOG_TTL: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(in crate::acp::builtin_mcp) struct CatalogCache {
    current: Mutex<Option<Arc<CatalogEntry>>>,
}

struct CatalogEntry {
    scope: String,
    result: OnceCell<Result<CatalogSnapshot, ErrorData>>,
}

struct CatalogSnapshot {
    models: Arc<Vec<Value>>,
    loaded_at: Instant,
}

impl CatalogCache {
    pub(super) async fn read(
        &self,
        (scope, previous): (&str, Option<&Arc<Vec<Value>>>),
        fetch: impl Future<Output = Result<Arc<Vec<Value>>, ErrorData>>,
    ) -> Result<Arc<Vec<Value>>, ErrorData> {
        let entry = self.entry(scope, previous).await;
        tracing::debug!(
            cache_hit = entry.result.initialized(),
            "[iyw-image] model catalog reuse lookup"
        );
        let result = entry
            .result
            .get_or_init(|| async {
                let models = fetch.await?;
                tracing::info!(
                    model_count = models.len(),
                    ttl_seconds = CATALOG_TTL.as_secs(),
                    "[iyw-image] shared model catalog refreshed"
                );
                Ok(CatalogSnapshot {
                    models,
                    loaded_at: Instant::now(),
                })
            })
            .await;
        match result {
            Ok(snapshot) => Ok(Arc::clone(&snapshot.models)),
            Err(error) => Err(error.clone()),
        }
    }

    async fn entry(&self, scope: &str, previous: Option<&Arc<Vec<Value>>>) -> Arc<CatalogEntry> {
        let mut current = self.current.lock().await;
        if current
            .as_ref()
            .is_none_or(|entry| entry.expired(scope, previous))
        {
            *current = Some(Arc::new(CatalogEntry {
                scope: scope.to_string(),
                result: OnceCell::new(),
            }));
        }
        Arc::clone(current.as_ref().expect("catalog entry initialized"))
    }
}

impl CatalogEntry {
    fn expired(&self, scope: &str, previous: Option<&Arc<Vec<Value>>>) -> bool {
        if self.scope != scope {
            return true;
        }
        match self.result.get() {
            None => false,
            Some(Err(_)) => true,
            Some(Ok(snapshot)) => {
                snapshot.loaded_at.elapsed() >= CATALOG_TTL
                    || previous.is_some_and(|models| Arc::ptr_eq(models, &snapshot.models))
            }
        }
    }
}
