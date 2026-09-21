use std::collections::BTreeMap;

use super::{
    UserMemoryRecallItem, UserMemoryRecallResult, UserMemoryRecallScope, UserMemoryRecallState,
    UserMemoryService,
};
use crate::app_error::AppCommandError;

const RESULT_LIMIT: usize = 6;
const MIN_SCORE: f32 = 0.60;
const RRF_OFFSET: f64 = 60.0;
const QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(300);

impl UserMemoryService {
    pub(super) async fn augment_semantic_recall(
        &self,
        mut result: UserMemoryRecallResult,
        scope: UserMemoryRecallScope,
        limit: usize,
    ) -> UserMemoryRecallResult {
        if result.result_state == UserMemoryRecallState::Unavailable {
            return result;
        }
        let query = result.query.clone();
        let outcome = tokio::time::timeout(QUERY_TIMEOUT, async {
            if !self.semantic_recall_enabled().await? {
                return Ok(None);
            }
            let items = self.semantic_items(&query, &scope).await?;
            let current = self.read_index_source().await?;
            Ok::<_, AppCommandError>(Some((items, current)))
        })
        .await;
        let Ok(Ok(Some((items, current)))) = outcome else {
            return result;
        };
        fuse(&mut result.items, items);
        let now = chrono::Utc::now();
        result.items.retain(|item| {
            current.items.iter().any(|source| {
                source.id == item.id
                    && source.source_revision == item.source_revision
                    && !source.sensitive
                    && scope.permits(&source.scope_type, &source.scope_key)
                    && super::recall_validity::item_is_current_at(source, &now)
            })
        });
        deduplicate_content(&mut result.items);
        result.items.truncate(limit);
        enforce_budget(&mut result.items);
        update_result_state(&mut result, current.source_digest);
        result
    }

    pub(super) async fn semantic_items(
        &self,
        query: &str,
        scope: &UserMemoryRecallScope,
    ) -> Result<Vec<UserMemoryRecallItem>, AppCommandError> {
        #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
        {
            let snapshot = self.read_index_source().await?;
            super::index_integrity::validate_snapshot_identities(&snapshot)
                .map_err(super::index_checkpoint::database_error)?;
            let expected = snapshot.source_digest.clone();
            let scope = self.memory_recall_scope(scope.clone());
            let matches = self
                .run_semantic_query(snapshot, query.to_string(), scope)
                .await?;
            let current = self.read_index_source().await?;
            if current.source_digest != expected {
                return Ok(Vec::new());
            }
            Ok(matches
                .into_iter()
                .filter_map(|hit| hydrate_hit(hit, &current.items))
                .take(RESULT_LIMIT)
                .collect())
        }
        #[cfg(not(all(feature = "memory-semantic", target_pointer_width = "64")))]
        {
            let _ = (query, scope);
            Ok(Vec::new())
        }
    }

    #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
    async fn run_semantic_query(
        &self,
        snapshot: super::index_types::IndexSnapshot,
        query: String,
        scope: UserMemoryRecallScope,
    ) -> Result<Vec<(String, String, f32)>, AppCommandError> {
        self.touch_semantic_activity();
        let permit = self
            .semantic
            .task
            .clone()
            .try_acquire_owned()
            .map_err(|_| super::helpers::conflict("Memory semantic worker is busy"))?;
        let runtime = self.semantic.clone();
        let service = self.clone();
        service.set_semantic_busy(true);
        tokio::task::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                service.query_semantic_index(&snapshot, &query, &scope)
            }))
            .unwrap_or_else(|_| {
                Err(super::semantic_model::model_error(
                    "Semantic worker interrupted",
                ))
            });
            if let Err(error) = &result {
                *runtime.index.lock().unwrap_or_else(|e| e.into_inner()) = None;
                service.finish_semantic(Err(error.clone()));
                service.schedule_missing_model_repair();
            }
            drop(permit);
            if runtime
                .refresh_requested
                .load(std::sync::atomic::Ordering::Acquire)
            {
                service.schedule_semantic_refresh();
            }
            result
        })
        .await
        .map_err(super::semantic_model::model_error)?
    }

    #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
    fn query_semantic_index(
        &self,
        snapshot: &super::index_types::IndexSnapshot,
        query: &str,
        scope: &UserMemoryRecallScope,
    ) -> Result<Vec<(String, String, f32)>, AppCommandError> {
        let mut index = self
            .semantic
            .index
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if index.is_none() {
            *index = Some(super::semantic_index::SemanticIndex::open(
                self.resolved_root()?,
            )?);
        }
        let index = index.as_mut().expect("initialized semantic index");
        index.synchronize(snapshot)?;
        let result = index.query(snapshot, query, scope)?;
        self.finish_semantic(Ok(index.indexed));
        Ok(result)
    }
}

fn deduplicate_content(items: &mut Vec<UserMemoryRecallItem>) {
    let mut unique = Vec::<UserMemoryRecallItem>::new();
    for item in std::mem::take(items) {
        if let Some(index) = unique
            .iter()
            .position(|existing| existing.content == item.content)
        {
            if unique[index].kind == "candidate" && item.kind != "candidate" {
                unique[index] = item;
            }
        } else {
            unique.push(item);
        }
    }
    *items = unique;
}

fn update_result_state(result: &mut UserMemoryRecallResult, digest: String) {
    if result.source_digest.as_deref() != Some(digest.as_str()) {
        result.index_generation = None;
    }
    result.source_digest = Some(digest);
    result
        .reason_codes
        .retain(|reason| reason != "recall_abstained");
    result.abstained = result.items.is_empty();
    result.result_state = if result.abstained {
        result.reason_codes.push("recall_abstained".into());
        UserMemoryRecallState::NoEvidence
    } else {
        UserMemoryRecallState::Matched
    };
}

fn hydrate_hit(
    hit: (String, String, f32),
    items: &[super::index_types::IndexItem],
) -> Option<UserMemoryRecallItem> {
    let (id, digest, score) = hit;
    let item = items
        .iter()
        .find(|item| item.id == id && item.content_digest == digest)?;
    if score < MIN_SCORE
        || item.sensitive
        || !super::recall_validity::item_is_current_at(item, &chrono::Utc::now())
    {
        return None;
    }
    let content = super::recall_types::bounded_recall_content(
        &item.content,
        super::recall_types::MAX_RECALL_ITEM_CHARS,
    )?;
    Some(UserMemoryRecallItem {
        id,
        kind: if item.trust_class == "candidate" {
            "candidate".into()
        } else {
            item.kind.clone()
        },
        content,
        confidence: item.confidence,
        importance: item.importance,
        source_revision: item.source_revision.clone(),
        score: f64::from(score),
        lanes: vec!["semantic".into()],
    })
}

fn fuse(existing: &mut Vec<UserMemoryRecallItem>, semantic: Vec<UserMemoryRecallItem>) {
    let mut scores = BTreeMap::<String, f64>::new();
    for (rank, item) in existing.iter().enumerate() {
        scores.insert(item.id.clone(), 1.0 / (RRF_OFFSET + rank as f64));
    }
    for (rank, item) in semantic.into_iter().enumerate() {
        *scores.entry(item.id.clone()).or_default() += 1.0 / (RRF_OFFSET + rank as f64);
        if let Some(previous) = existing.iter_mut().find(|previous| previous.id == item.id) {
            if !previous.lanes.iter().any(|lane| lane == "semantic") {
                previous.lanes.push("semantic".into());
            }
        } else {
            existing.push(item);
        }
    }
    existing.sort_by(|left, right| {
        scores[&right.id]
            .total_cmp(&scores[&left.id])
            .then_with(|| semantic_hit(right).cmp(&semantic_hit(left)))
    });
}

fn semantic_hit(item: &UserMemoryRecallItem) -> bool {
    item.lanes.iter().any(|lane| lane == "semantic")
}

fn enforce_budget(items: &mut Vec<UserMemoryRecallItem>) {
    let mut remaining = super::recall_types::MAX_RECALL_TOTAL_CHARS;
    items.retain(|item| {
        let length = item.content.chars().count();
        if length > remaining {
            return false;
        }
        remaining -= length;
        true
    });
}
