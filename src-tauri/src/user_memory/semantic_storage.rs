use std::{collections::BTreeSet, path::Path};

use qdrant_edge::{EdgeConfig, EdgeShard, PointId, ScrollRequest, WithPayloadInterface};

use super::semantic_model::model_error;
use crate::app_error::AppCommandError;

const SCROLL_BATCH_SIZE: usize = 256;
const WRITER_LOCK: &str = "writer.lock";

pub(super) fn load(
    directory: &Path,
    config: EdgeConfig,
) -> Result<(EdgeShard, BTreeSet<PointId>), AppCommandError> {
    match load_existing(directory, config.clone()) {
        Ok(result) => Ok(result),
        Err(error) => {
            tracing::warn!(code = ?error.code, "[memory-semantic] rebuilding unreadable derived index");
            clear_projection(directory)?;
            let shard = EdgeShard::new(directory, config).map_err(model_error)?;
            Ok((shard, BTreeSet::new()))
        }
    }
}

fn load_existing(
    directory: &Path,
    config: EdgeConfig,
) -> Result<(EdgeShard, BTreeSet<PointId>), AppCommandError> {
    let shard = EdgeShard::load(directory, Some(config)).map_err(model_error)?;
    let mut known = BTreeSet::new();
    let mut offset = None;
    loop {
        let (points, next) = shard
            .scroll(ScrollRequest {
                offset,
                limit: Some(SCROLL_BATCH_SIZE),
                with_payload: Some(WithPayloadInterface::Bool(false)),
                ..Default::default()
            })
            .map_err(model_error)?;
        known.extend(points.into_iter().map(|point| point.id));
        if next.is_none() {
            return Ok((shard, known));
        }
        offset = next;
    }
}

fn clear_projection(directory: &Path) -> Result<(), AppCommandError> {
    // 调用方持有 writer.lock；仅清理由权威记忆可重建的向量目录。
    super::helpers::reject_symlink(directory)?;
    let entries = std::fs::read_dir(directory)
        .map_err(AppCommandError::io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AppCommandError::io)?;
    for entry in entries {
        if entry.file_name() == WRITER_LOCK {
            continue;
        }
        let path = entry.path();
        super::helpers::reject_symlink(&path)?;
        if entry.file_type().map_err(AppCommandError::io)?.is_dir() {
            std::fs::remove_dir_all(path).map_err(AppCommandError::io)?;
        } else {
            std::fs::remove_file(path).map_err(AppCommandError::io)?;
        }
    }
    Ok(())
}
