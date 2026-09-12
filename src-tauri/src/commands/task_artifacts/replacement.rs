use std::path::Path;

use tempfile::TempDir;

use crate::acp::delegation::artifact_tool::ArtifactContext;
use crate::db::error::DbError;
use crate::db::service::task_artifact_service::source::{resolve_sources, ResolvedArtifact};

pub(super) struct Replacement {
    pub source: ResolvedArtifact,
    pub directory: Option<TempDir>,
}

pub(super) async fn prepare(
    context: &ArtifactContext,
    source: String,
) -> Result<Replacement, DbError> {
    let resolved = resolve_one(&context.working_dir, source)?;
    if resolved.kind == "url" {
        return Ok(Replacement {
            source: resolved,
            directory: None,
        });
    }
    let generation = context
        .turn_generation
        .filter(|value| *value > 0)
        .ok_or_else(|| DbError::Validation("managed_directory_unavailable".into()))?;
    let directory = crate::acp::task_artifact_delivery::ensure_managed_turn_directory(
        &context.connection_id,
        context.conversation_id,
        generation,
    )
    .await?;
    tokio::task::spawn_blocking(move || prepare_local(&directory, resolved))
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "[task-artifacts] replacement preparation task failed");
            DbError::Validation("materialize_failed".into())
        })?
}

fn prepare_local(directory: &Path, source: ResolvedArtifact) -> Result<Replacement, DbError> {
    let source_path = Path::new(&source.path);
    let name = source_path
        .file_name()
        .ok_or_else(|| DbError::Validation("invalid_path".into()))?;
    let staging = tempfile::Builder::new()
        .prefix("update-")
        .tempdir_in(directory)?;
    let target = staging.path().join(name);
    let canonical_staging = std::fs::canonicalize(staging.path())?;
    if canonical_staging.starts_with(std::fs::canonicalize(source_path)?) {
        return Err(DbError::Validation("recursive_source".into()));
    }
    copy_tree(source_path, &target)?;
    let mut resolved = resolve_one(staging.path(), target.to_string_lossy().into_owned())?;
    resolved.source = source.source;
    Ok(Replacement {
        source: resolved,
        directory: Some(staging),
    })
}

fn resolve_one(working_dir: &Path, source: String) -> Result<ResolvedArtifact, DbError> {
    let (mut resolved, rejected) = resolve_sources(working_dir, vec![source]);
    resolved.pop().ok_or_else(|| {
        DbError::Validation(
            rejected
                .into_iter()
                .next()
                .map(|(_, reason)| reason)
                .unwrap_or_else(|| "invalid_source".into()),
        )
    })
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), DbError> {
    let mut pending = vec![(source.to_owned(), target.to_owned())];
    while let Some((source, target)) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&source)?;
        if is_link(&metadata) {
            return Err(DbError::Validation("symlink_source".into()));
        }
        if metadata.is_file() {
            std::fs::copy(source, target)?;
            continue;
        }
        if !metadata.is_dir() {
            return Err(DbError::Validation("unsupported_type".into()));
        }
        std::fs::create_dir(&target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            pending.push((entry.path(), target.join(entry.file_name())));
        }
    }
    Ok(())
}

fn is_link(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}
