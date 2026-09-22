use std::path::Path;

use super::{authority_sql as sql, UserMemoryService};
use crate::app_error::AppCommandError;

impl UserMemoryService {
    pub(super) async fn clear_forgotten_vector_projection(&self) -> Result<(), AppCommandError> {
        #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
        self.drop_semantic_index().await?;
        let root = self.resolved_root()?.join(".memory-vector");
        tokio::task::spawn_blocking(move || clear_vector_files(&root))
            .await
            .map_err(|_| {
                AppCommandError::task_execution_failed("Memory index cleanup interrupted")
            })?
    }
}

fn clear_vector_files(root: &Path) -> Result<(), AppCommandError> {
    super::helpers::reject_symlink(root)?;
    if !root.try_exists().map_err(AppCommandError::io)? {
        return Ok(());
    }
    let mut directories = Vec::new();
    for entry in std::fs::read_dir(root).map_err(AppCommandError::io)? {
        let entry = entry.map_err(AppCommandError::io)?;
        let path = entry.path();
        super::helpers::reject_symlink(&path)?;
        if !entry.file_type().map_err(AppCommandError::io)?.is_dir() {
            return Err(AppCommandError::configuration_invalid(
                "Unexpected vector cache entry",
            ));
        }
        let lock = super::platform::open_lock_no_follow(&path.join("writer.lock"))
            .map_err(AppCommandError::io)?;
        lock.try_lock().map_err(|error| {
            super::helpers::conflict("Memory vector index is in use").with_detail(error.to_string())
        })?;
        directories.push((path, lock));
    }
    for (directory, _lock) in &directories {
        clear_directory(directory)?;
    }
    Ok(())
}

fn clear_directory(directory: &Path) -> Result<(), AppCommandError> {
    for entry in std::fs::read_dir(directory).map_err(AppCommandError::io)? {
        let entry = entry.map_err(AppCommandError::io)?;
        if entry.file_name() == "writer.lock" {
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

pub(super) async fn clear_fts<C: sea_orm::ConnectionTrait>(db: &C) -> Result<(), AppCommandError> {
    for table in ["memory_item_fts_unicode", "memory_item_fts_trigram"] {
        let present = sql::rows(
            db,
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?",
            vec![table.into()],
        )
        .await?;
        if !present.is_empty() {
            sql::execute(
                db,
                &format!("INSERT INTO {table}({table}) VALUES ('delete-all')"),
                vec![],
            )
            .await?;
        }
    }
    for table in [
        "memory_relation_current",
        "memory_evidence",
        "memory_alias_current",
        "memory_item_current",
    ] {
        sql::execute(db, &format!("DELETE FROM {table}"), vec![]).await?;
    }
    sql::execute(
        db,
        "UPDATE memory_source_checkpoint SET status='stale',last_error='memory_forgotten'",
        vec![],
    )
    .await?;
    Ok(())
}
