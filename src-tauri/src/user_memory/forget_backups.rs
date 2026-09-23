use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sea_orm::ConnectionTrait;

use crate::app_error::AppCommandError;

use super::{authority_sql as sql, UserMemoryService};

const MAX_BACKUP_FILES: usize = 2_048;
const MAX_TEXT_BACKUP_BYTES: u64 = 67_108_864;

pub(super) struct BackupForgetResult {
    pub purged: Vec<String>,
    pub residual: Vec<String>,
}

pub(super) async fn prepare(
    service: &UserMemoryService,
    target: (&str, &[String]),
) -> Result<BTreeSet<PathBuf>, AppCommandError> {
    matching_paths(service, target).await
}

pub(super) async fn prepare_clear(
    service: &UserMemoryService,
    needles: &[String],
    all: bool,
) -> Result<BTreeSet<PathBuf>, AppCommandError> {
    if all {
        return Ok(backup_files(service).await?.into_iter().collect());
    }
    if needles.is_empty() {
        return Ok(BTreeSet::new());
    }
    matching_paths(service, ("", needles)).await
}

pub(super) fn finish(paths: BTreeSet<PathBuf>, purge: bool) -> BackupForgetResult {
    if !purge {
        return BackupForgetResult {
            purged: Vec::new(),
            residual: display(paths),
        };
    }
    let mut purged = Vec::new();
    let mut residual = Vec::new();
    for path in expand_pairs(paths) {
        match std::fs::remove_file(&path) {
            Ok(()) => purged.push(path.to_string_lossy().into_owned()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => residual.push(path.to_string_lossy().into_owned()),
        }
    }
    BackupForgetResult { purged, residual }
}

async fn matching_paths(
    service: &UserMemoryService,
    target: (&str, &[String]),
) -> Result<BTreeSet<PathBuf>, AppCommandError> {
    let files = backup_files(service).await?;
    let mut matched = BTreeSet::new();
    for path in files {
        let contains = if path.extension().is_some_and(|value| value == "db") {
            database_contains(&path, target).await.unwrap_or(true)
        } else {
            text_contains(&path, target).unwrap_or(true)
        };
        if contains {
            matched.insert(path);
        }
    }
    Ok(matched)
}

async fn backup_files(service: &UserMemoryService) -> Result<Vec<PathBuf>, AppCommandError> {
    let mut files = root_backup_files(service.resolved_root()?)?;
    files.extend(restore_backup_files(service).await?);
    if files.len() > MAX_BACKUP_FILES {
        return Err(AppCommandError::configuration_invalid(
            "Too many memory backups to inspect safely",
        ));
    }
    Ok(files)
}

fn root_backup_files(root: &Path) -> Result<Vec<PathBuf>, AppCommandError> {
    super::helpers::reject_symlink(root)?;
    Ok(std::fs::read_dir(root)
        .map_err(AppCommandError::io)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(".memory-authority-backup-") || name.starts_with(".memory-conflict-")
        })
        .map(|entry| entry.path())
        .collect())
}

async fn restore_backup_files(
    service: &UserMemoryService,
) -> Result<Vec<PathBuf>, AppCommandError> {
    let rows = sql::rows(&service.db, "PRAGMA database_list", vec![]).await?;
    let database = rows
        .iter()
        .find(|row| sql::field::<String>(row, "name").ok().as_deref() == Some("main"))
        .map(|row| sql::field::<String>(row, "file"))
        .transpose()?
        .map(PathBuf::from);
    let Some(parent) = database.as_deref().and_then(Path::parent) else {
        return Ok(Vec::new());
    };
    let safety = parent.join(crate::commands::backup::restore::SAFETY_DIR);
    super::helpers::reject_symlink(&safety)?;
    if !safety.try_exists().map_err(AppCommandError::io)? {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for directory in std::fs::read_dir(safety).map_err(AppCommandError::io)? {
        let Ok(directory) = directory else { continue };
        if !directory.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        super::helpers::reject_symlink(&directory.path())?;
        for entry in std::fs::read_dir(directory.path()).map_err(AppCommandError::io)? {
            let Ok(entry) = entry else { continue };
            if entry.file_type().is_ok_and(|kind| kind.is_file())
                && is_memory_backup_file(&entry.file_name().to_string_lossy())
            {
                files.push(entry.path());
            }
        }
    }
    Ok(files)
}

fn is_memory_backup_file(name: &str) -> bool {
    name == crate::db::database_file_name()
        || matches!(
            name,
            "user-memory.md"
                | "user-profile.md"
                | "user-soul.md"
                | ".user-memory-learning.json"
                | ".memory-authority.json"
        )
        || name.starts_with(".memory-authority-backup-")
        || name.starts_with(".memory-conflict-")
}

async fn database_contains(
    path: &Path,
    target: (&str, &[String]),
) -> Result<bool, AppCommandError> {
    super::helpers::reject_symlink(path)?;
    let mut options = sea_orm::ConnectOptions::new("sqlite::memory:");
    let database_path = path.to_path_buf();
    options
        .max_connections(1)
        .min_connections(1)
        .sqlx_logging(false);
    options.map_sqlx_sqlite_opts(move |sqlite| {
        sqlite
            .filename(database_path.clone())
            .in_memory(false)
            .read_only(true)
            .create_if_missing(false)
    });
    let db = sea_orm::Database::connect(options)
        .await
        .map_err(super::index_checkpoint::database_error)?;
    let result = database_contains_open(&db, target).await;
    db.close()
        .await
        .map_err(super::index_checkpoint::database_error)?;
    result
}

async fn database_contains_open<C: ConnectionTrait>(
    db: &C,
    target: (&str, &[String]),
) -> Result<bool, AppCommandError> {
    let tables = sql::rows(
        db,
        "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('memory_authority','memory_revision')",
        vec![],
    )
    .await?
    .into_iter()
    .filter_map(|row| sql::field::<String>(&row, "name").ok())
    .collect::<BTreeSet<_>>();
    for needle in std::iter::once(target.0)
        .chain(target.1.iter().map(String::as_str))
        .filter(|text| !text.is_empty())
    {
        if tables.contains("memory_authority")
            && !sql::rows(
                db,
                "SELECT 1 AS found FROM memory_authority WHERE instr(snapshot_json,?)>0 LIMIT 1",
                vec![needle.into()],
            )
            .await?
            .is_empty()
        {
            return Ok(true);
        }
        if tables.contains("memory_revision")
            && !sql::rows(
                db,
                "SELECT 1 AS found FROM memory_revision WHERE instr(item_json,?)>0 LIMIT 1",
                vec![needle.into()],
            )
            .await?
            .is_empty()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn text_contains(path: &Path, target: (&str, &[String])) -> Result<bool, AppCommandError> {
    super::helpers::reject_symlink(path)?;
    let metadata = std::fs::metadata(path).map_err(AppCommandError::io)?;
    if metadata.len() > MAX_TEXT_BACKUP_BYTES {
        return Ok(true);
    }
    let bytes = std::fs::read(path).map_err(AppCommandError::io)?;
    Ok(std::iter::once(target.0.as_bytes())
        .chain(target.1.iter().map(String::as_bytes))
        .any(|needle| contains_bytes(&bytes, needle)))
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn expand_pairs(paths: BTreeSet<PathBuf>) -> BTreeSet<PathBuf> {
    let mut expanded = paths.clone();
    for path in paths {
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if stem.starts_with(".memory-authority-backup-") {
            expanded.insert(
                path.with_extension(if path.extension().is_some_and(|v| v == "db") {
                    "json"
                } else {
                    "db"
                }),
            );
        }
    }
    expanded
}

fn display(paths: BTreeSet<PathBuf>) -> Vec<String> {
    expand_pairs(paths)
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}
