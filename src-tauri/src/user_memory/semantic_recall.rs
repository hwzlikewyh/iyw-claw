use std::collections::BTreeMap;

use super::{
    UserMemoryRecallItem, UserMemoryRecallResult, UserMemoryRecallScope, UserMemoryRecallState,
    UserMemoryService,
};
use crate::app_error::AppCommandError;

const RESULT_LIMIT: usize = 6;
const MIN_SCORE: f32 = 0.60;
const RRF_OFFSET: f64 = 60.0;
const QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
const PREFETCH_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(40);

impl UserMemoryService {
    pub(super) async fn augment_semantic_recall(
        &self,
        mut result: UserMemoryRecallResult,
        options: (UserMemoryRecallScope, usize, bool),
    ) -> UserMemoryRecallResult {
        let (scope, limit, prefetch) = options;
        if result.result_state == UserMemoryRecallState::Unavailable {
            return result;
        }
        let query = result.query.clone();
        let timeout = if prefetch {
            PREFETCH_QUERY_TIMEOUT
        } else {
            QUERY_TIMEOUT
        };
        let outcome = tokio::time::timeout(timeout, async {
            if !self.semantic_recall_enabled().await? {
                return Ok(None);
            }
            let items = self.semantic_items(&query, &scope).await?;
            Ok::<_, AppCommandError>(Some(items))
        })
        .await;
        if let Ok(Ok(Some(items))) = outcome {
            fuse(&mut result.items, items);
        }
        deduplicate_content(&mut result.items);
        if !prefetch {
            self.rerank_memory_items((&query, &scope), &mut result.items)
                .await;
        }
        self.validate_augmented_recall(result, (scope, limit)).await
    }

    async fn validate_augmented_recall(
        &self,
        mut result: UserMemoryRecallResult,
        options: (UserMemoryRecallScope, usize),
    ) -> UserMemoryRecallResult {
        let (scope, limit) = options;
        let Ok(current) = self.read_index_source().await else {
            result.items.clear();
            result.abstained = true;
            result.result_state = UserMemoryRecallState::Unavailable;
            result.reason_codes.push("memory_source_unavailable".into());
            return result;
        };
        retain_current(&mut result.items, &current, &scope);
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
            let mut seen = std::collections::BTreeSet::new();
            Ok(matches
                .into_iter()
                .filter_map(|hit| hydrate_hit(hit, &current.items))
                .filter(|item| seen.insert(item.id.clone()))
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
        let gateway = self.cloud_gateway().await?;
        let generation = self
            .semantic
            .generation
            .load(std::sync::atomic::Ordering::Acquire);
        let config = self.cloud_retrieval_config().await?;
        if config.embedding_model.is_empty() {
            self.schedule_semantic_refresh();
            return Ok(Vec::new());
        }
        let identity = super::semantic_cloud_index::index_identity(&gateway, &config);
        let vector = self
            .cloud_query_vector(gateway, config.embedding_model, query)
            .await?;
        let model_identity = identity;
        let identity = super::semantic_cloud_index::space_identity(&model_identity, &vector.space);
        let result = self
            .query_cloud_index(super::semantic_cloud_index::CloudSearch {
                snapshot,
                scope,
                identity,
                model_identity,
                generation,
                vector: vector.values,
            })
            .await;
        self.schedule_semantic_refresh();
        result
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

pub(super) fn retain_current(
    items: &mut Vec<UserMemoryRecallItem>,
    snapshot: &super::index_types::IndexSnapshot,
    scope: &UserMemoryRecallScope,
) {
    let now = chrono::Utc::now();
    let allowed = snapshot
        .items
        .iter()
        .filter(|source| {
            !source.sensitive
                && scope.permits(&source.scope_type, &source.scope_key)
                && super::recall_validity::item_is_current_at(source, &now)
        })
        .map(|source| (source.id.as_str(), source.source_revision.as_str()))
        .collect::<BTreeMap<_, _>>();
    let conflicts = snapshot
        .relations
        .iter()
        .filter(|relation| {
            relation.relation == "contradicts"
                && allowed.contains_key(relation.source_id.as_str())
                && allowed.contains_key(relation.target_id.as_str())
        })
        .flat_map(|relation| [relation.source_id.as_str(), relation.target_id.as_str()])
        .collect::<std::collections::BTreeSet<_>>();
    items.retain(|item| {
        allowed.get(item.id.as_str()) == Some(&item.source_revision.as_str())
            && !conflicts.contains(item.id.as_str())
    });
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
