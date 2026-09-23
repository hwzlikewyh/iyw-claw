use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::app_error::AppCommandError;

use super::restore::{ConflictPolicy, ExternalRestoreMode};

pub(super) const PLAN_FILE: &str = ".restore-external.json";

#[derive(Serialize, Deserialize)]
struct RestorePlan {
    mode: ExternalRestoreMode,
    roots: BTreeMap<String, PathBuf>,
    side_path: PathBuf,
    safety_path: PathBuf,
}

pub(super) fn prepare(
    staging: &Path,
    data_dir: &Path,
    mode: ExternalRestoreMode,
) -> Result<(Option<String>, Vec<String>), AppCommandError> {
    let sources = super::external::sources();
    let roots = sources
        .iter()
        .map(|source| (source.agent.to_string(), source.root.clone()))
        .collect();
    let side_path = data_dir
        .join(super::restore::RESTORED_TRANSCRIPTS_DIR)
        .join(uuid::Uuid::new_v4().to_string());
    let safety_path = data_dir
        .join(super::restore::SAFETY_DIR)
        .join(format!("external-{}", uuid::Uuid::new_v4()));
    let skipped = if matches!(
        mode,
        ExternalRestoreMode::OriginalLocations {
            on_conflict: ConflictPolicy::SkipExisting
        }
    ) {
        super::external::staged_conflicts(&staging.join("external"), &sources)?
    } else {
        Vec::new()
    };
    let side = (mode == ExternalRestoreMode::SideLocation && staging.join("external").is_dir())
        .then(|| side_path.to_string_lossy().into_owned());
    let bytes = serde_json::to_vec(&RestorePlan {
        mode,
        roots,
        side_path,
        safety_path,
    })
    .map_err(|error| AppCommandError::invalid_input(error.to_string()))?;
    std::fs::write(staging.join(PLAN_FILE), bytes).map_err(AppCommandError::io)?;
    Ok((side, skipped))
}

pub(super) fn apply(staging: &Path) -> Result<(), AppCommandError> {
    let path = staging.join(PLAN_FILE);
    let external = staging.join("external");
    if !path.is_file() || !external.is_dir() {
        return Ok(());
    }
    let plan: RestorePlan =
        serde_json::from_slice(&std::fs::read(path).map_err(AppCommandError::io)?)
            .map_err(|error| AppCommandError::invalid_input(error.to_string()))?;
    match plan.mode {
        ExternalRestoreMode::Skip => Ok(()),
        ExternalRestoreMode::SideLocation => {
            super::restore::move_path(&external, &plan.side_path).map_err(AppCommandError::io)
        }
        ExternalRestoreMode::OriginalLocations { on_conflict } => {
            let mut sources = super::external::sources();
            for source in &mut sources {
                source.root = plan
                    .roots
                    .get(source.agent)
                    .cloned()
                    .ok_or_else(super::unknown_format_error)?;
            }
            if on_conflict == ConflictPolicy::Overwrite {
                backup_existing(&external, &sources, &plan.safety_path)?;
            }
            let skipped = super::external::restore_external_with_sources(
                &external,
                &sources,
                on_conflict,
                &CancellationToken::new(),
            )?;
            tracing::info!(
                skipped_files = skipped.len(),
                "[RESTORE] native session files restored before startup"
            );
            Ok(())
        }
    }
}

fn backup_existing(
    external: &Path,
    sources: &[crate::parsers::ExternalSource],
    safety: &Path,
) -> Result<(), AppCommandError> {
    for raw in super::external::staged_conflicts(external, sources)? {
        let target = PathBuf::from(raw);
        if !std::fs::symlink_metadata(&target)
            .map_err(AppCommandError::io)?
            .file_type()
            .is_file()
        {
            return Err(AppCommandError::invalid_input(
                "Existing transcript destination is not a regular file",
            ));
        }
        let source = sources
            .iter()
            .find(|source| {
                if source.is_file {
                    target == source.root
                } else {
                    target.starts_with(&source.root)
                }
            })
            .ok_or_else(super::unknown_format_error)?;
        super::external_write::require_closed_database(&target)?;
        let base = source.restore_base();
        let relative = target
            .strip_prefix(&base)
            .map_err(|_| super::unknown_format_error())?;
        let backup = safety.join(source.agent).join(relative);
        super::external_write::restore_file(
            &target,
            (safety, &backup),
            ConflictPolicy::SkipExisting,
        )?;
    }
    Ok(())
}
