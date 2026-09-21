use std::collections::BTreeSet;
use std::path::Path;

use fastembed::TextEmbedding;
use qdrant_edge::{
    Condition, Distance, EdgeConfig, EdgeShard, EdgeVectorParams, Filter, HasIdCondition,
    NamedQuery, PointId, PointInsertOperations, PointOperations, PointStruct, QueryEnum,
    QueryRequestBuilder, ScoringQuery, UpdateOperation, VectorInternal, WithPayloadInterface,
    DEFAULT_VECTOR_NAME,
};

use super::index_types::{IndexItem, IndexSnapshot};
use super::semantic_model::{self, model_error};
use super::UserMemoryRecallScope;
use crate::app_error::AppCommandError;

const MAX_SEMANTIC_RESULTS: usize = 20;
const EMBEDDING_BATCH_SIZE: usize = 16;

pub(super) struct SemanticIndex {
    shard: EdgeShard,
    model: TextEmbedding,
    pub digest: String,
    pub indexed: usize,
    known_points: BTreeSet<PointId>,
    _writer_lock: std::fs::File,
}

impl SemanticIndex {
    pub fn open(root: &Path) -> Result<Self, AppCommandError> {
        super::helpers::reject_symlink(&root.join(".memory-vector"))?;
        let directory = root.join(".memory-vector").join(semantic_model::MODEL_ID);
        super::helpers::reject_symlink(&directory)?;
        std::fs::create_dir_all(&directory).map_err(AppCommandError::io)?;
        let lock = super::platform::open_lock_no_follow(&directory.join("writer.lock"))
            .map_err(AppCommandError::io)?;
        lock.try_lock().map_err(model_error)?;
        let config = EdgeConfig {
            vectors: [(
                DEFAULT_VECTOR_NAME.into(),
                EdgeVectorParams::builder(semantic_model::DIMENSION, Distance::Cosine).build(),
            )]
            .into(),
            max_search_threads: Some(1),
            ..Default::default()
        };
        let model = semantic_model::load(root)?;
        let (shard, known_points) = super::semantic_storage::load(&directory, config)?;
        Ok(Self {
            _writer_lock: lock,
            shard,
            model,
            digest: String::new(),
            indexed: 0,
            known_points,
        })
    }

    pub fn synchronize(&mut self, source: &IndexSnapshot) -> Result<(), AppCommandError> {
        let now = chrono::Utc::now();
        let items = source
            .items
            .iter()
            .filter(|item| {
                !item.sensitive && super::recall_validity::item_is_current_at(item, &now)
            })
            .collect::<Vec<_>>();
        let point_ids = items
            .iter()
            .map(|item| point_id(&item.id, &item.content_digest))
            .collect::<BTreeSet<_>>();
        if self.digest == source.source_digest && point_ids == self.known_points {
            return Ok(());
        }
        let changed = items
            .iter()
            .copied()
            .filter(|item| {
                !self
                    .known_points
                    .contains(&point_id(&item.id, &item.content_digest))
            })
            .collect::<Vec<_>>();
        self.upsert_items(&changed)?;
        let keep: HasIdCondition = point_ids.iter().copied().collect();
        self.shard
            .update(UpdateOperation::PointOperation(
                PointOperations::DeletePointsByFilter(Filter::new_must_not(Condition::HasId(keep))),
            ))
            .map_err(model_error)?;
        self.shard.flush().map_err(model_error)?;
        self.known_points = point_ids;
        self.digest = source.source_digest.clone();
        self.indexed = items.len();
        Ok(())
    }

    fn upsert_items(&mut self, changed: &[&IndexItem]) -> Result<(), AppCommandError> {
        for batch in changed.chunks(EMBEDDING_BATCH_SIZE) {
            let texts = batch
                .iter()
                .map(|item| item.content.as_str())
                .collect::<Vec<_>>();
            let vectors = self
                .model
                .embed(texts, Some(EMBEDDING_BATCH_SIZE))
                .map_err(model_error)?;
            let points = batch.iter().zip(vectors).map(|(item, vector)| {
                PointStruct::new(point_id(&item.id, &item.content_digest), vector,
                    serde_json::json!({"memory_id":item.id,"revision":item.source_revision,
                        "content_digest":item.content_digest,"scope_type":item.scope_type,"scope_key":item.scope_key}))
                    .into()
            }).collect();
            self.shard
                .update(UpdateOperation::PointOperation(
                    PointOperations::UpsertPoints(PointInsertOperations::PointsList(points)),
                ))
                .map_err(model_error)?;
        }
        Ok(())
    }

    pub fn query(
        &mut self,
        source: &IndexSnapshot,
        query: &str,
        scope: &UserMemoryRecallScope,
    ) -> Result<Vec<(String, String, f32)>, AppCommandError> {
        let allowed = allowed_points(source, scope);
        if allowed.is_empty() {
            return Ok(Vec::new());
        }
        let mut vectors = self
            .model
            .embed(
                vec![format!("为这个句子生成表示以用于检索相关文章：{query}")],
                Some(1),
            )
            .map_err(model_error)?;
        let vector = vectors
            .pop()
            .ok_or_else(|| model_error("Embedding result missing"))?;
        let request = QueryRequestBuilder::new(MAX_SEMANTIC_RESULTS)
            .filter(Filter::new_must(Condition::HasId(
                allowed.into_iter().collect(),
            )))
            .query(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery {
                query: VectorInternal::from(vector),
                using: Some(DEFAULT_VECTOR_NAME.into()),
            })))
            .with_payload(WithPayloadInterface::Bool(true))
            .build();
        let results = self.shard.query(request).map_err(model_error)?;
        Ok(results
            .into_iter()
            .filter_map(|point| {
                let payload = point.payload?;
                Some((
                    payload.0.get("memory_id")?.as_str()?.to_string(),
                    payload.0.get("content_digest")?.as_str()?.to_string(),
                    point.score,
                ))
            })
            .collect())
    }
}

fn allowed_points(source: &IndexSnapshot, scope: &UserMemoryRecallScope) -> BTreeSet<PointId> {
    let now = chrono::Utc::now();
    let eligible = source
        .items
        .iter()
        .filter(|item| {
            !item.sensitive
                && scope.permits(&item.scope_type, &item.scope_key)
                && super::recall_validity::item_is_current_at(item, &now)
        })
        .collect::<Vec<_>>();
    let ids = eligible
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    let conflicts = source
        .relations
        .iter()
        .filter(|relation| {
            relation.relation == "contradicts"
                && ids.contains(relation.source_id.as_str())
                && ids.contains(relation.target_id.as_str())
        })
        .flat_map(|relation| [relation.source_id.as_str(), relation.target_id.as_str()])
        .collect::<BTreeSet<_>>();
    eligible
        .into_iter()
        .filter(|item| !conflicts.contains(item.id.as_str()))
        .map(|item| point_id(&item.id, &item.content_digest))
        .collect()
}

fn point_id(id: &str, digest: &str) -> PointId {
    let fingerprint = super::helpers::hash_parts(&[id.as_bytes(), digest.as_bytes()]);
    PointId::Uuid(uuid::Uuid::parse_str(&fingerprint[..32]).expect("hex digest is a valid UUID"))
}
