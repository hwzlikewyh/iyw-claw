use std::collections::BTreeSet;
use std::path::Path;

use qdrant_edge::{
    Condition, Distance, EdgeConfig, EdgeShard, EdgeVectorParams, Filter, HasIdCondition,
    NamedQuery, PointId, PointInsertOperations, PointOperations, PointStruct, QueryEnum,
    QueryRequestBuilder, ScoringQuery, UpdateOperation, VectorInternal, WithPayloadInterface,
    DEFAULT_VECTOR_NAME,
};

use super::index_types::IndexSnapshot;
use super::semantic_model::model_error;
use super::UserMemoryRecallScope;
use crate::app_error::AppCommandError;

const MAX_SEMANTIC_RESULTS: usize = 20;

pub(super) struct SemanticIndex {
    shard: EdgeShard,
    pub identity: String,
    pub model_identity: String,
    pub dimension: usize,
    pub digest: String,
    pub indexed: usize,
    known_points: BTreeSet<PointId>,
    _writer_lock: std::fs::File,
}

impl SemanticIndex {
    pub fn open(root: &Path, identity: &str, dimension: usize) -> Result<Self, AppCommandError> {
        super::helpers::reject_symlink(&root.join(".memory-vector"))?;
        let directory = root
            .join(".memory-vector")
            .join(format!("cloud-{identity}-{dimension}"));
        super::helpers::reject_symlink(&directory)?;
        std::fs::create_dir_all(&directory).map_err(AppCommandError::io)?;
        let lock = super::platform::open_lock_no_follow(&directory.join("writer.lock"))
            .map_err(AppCommandError::io)?;
        lock.try_lock().map_err(model_error)?;
        let config = EdgeConfig {
            vectors: [(
                DEFAULT_VECTOR_NAME.into(),
                EdgeVectorParams::builder(dimension, Distance::Cosine).build(),
            )]
            .into(),
            max_search_threads: Some(1),
            ..Default::default()
        };
        let (shard, known_points) = super::semantic_storage::load(&directory, config)?;
        Ok(Self {
            _writer_lock: lock,
            shard,
            identity: identity.to_string(),
            model_identity: String::new(),
            dimension,
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
            .flat_map(|item| super::semantic_chunks::point_ids(item))
            .collect::<BTreeSet<_>>();
        if self.digest == source.source_digest && point_ids == self.known_points {
            return Ok(());
        }
        let keep: HasIdCondition = point_ids.iter().copied().collect();
        self.shard
            .update(UpdateOperation::PointOperation(
                PointOperations::DeletePointsByFilter(Filter::new_must_not(Condition::HasId(keep))),
            ))
            .map_err(model_error)?;
        self.shard.flush().map_err(model_error)?;
        self.known_points.retain(|id| point_ids.contains(id));
        self.digest = source.source_digest.clone();
        self.indexed = items
            .iter()
            .filter(|item| {
                super::semantic_chunks::point_ids(item)
                    .iter()
                    .all(|id| self.known_points.contains(id))
            })
            .count();
        Ok(())
    }

    pub fn contains(&self, id: &PointId) -> bool {
        self.known_points.contains(id)
    }

    pub fn insert(
        &mut self,
        chunk: &super::semantic_chunks::MemoryChunk,
        vector: Vec<f32>,
    ) -> Result<(), AppCommandError> {
        if vector.len() != self.dimension {
            return Err(model_error("Embedding dimensions changed"));
        }
        let point = PointStruct::new(
            chunk.id,
            vector,
            serde_json::json!({"memory_id":chunk.memory_id,"content_digest":chunk.digest}),
        )
        .into();
        self.shard
            .update(UpdateOperation::PointOperation(
                PointOperations::UpsertPoints(PointInsertOperations::PointsList(vec![point])),
            ))
            .map_err(model_error)?;
        self.shard.flush().map_err(model_error)?;
        self.known_points.insert(chunk.id);
        Ok(())
    }

    pub fn query(
        &mut self,
        source: &IndexSnapshot,
        vector: Vec<f32>,
        scope: &UserMemoryRecallScope,
    ) -> Result<Vec<(String, String, f32)>, AppCommandError> {
        let allowed = allowed_points(source, scope);
        if allowed.is_empty() {
            return Ok(Vec::new());
        }
        if vector.len() != self.dimension {
            return Err(model_error("Embedding dimensions changed"));
        }
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
        .flat_map(super::semantic_chunks::point_ids)
        .collect()
}
