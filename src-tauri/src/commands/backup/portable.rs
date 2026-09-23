use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

pub(super) const PATHS_ENTRY: &str = "backup-paths.json";
const PATHS_VERSION: u32 = 1;
const MAX_PATHS_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct BackupPaths {
    version: u32,
    roots: BTreeMap<String, PathBuf>,
}

pub(super) fn host_roots(data_dir: &Path) -> BTreeMap<String, PathBuf> {
    BTreeMap::from([
        (
            "acp-transcripts".into(),
            crate::paths::iyw_claw_acp_transcripts_root(),
        ),
        ("chat-sessions".into(), data_dir.join("chat-sessions")),
        (
            "conversation-attachments".into(),
            data_dir.join("conversation-attachments"),
        ),
        (
            "task-artifacts".into(),
            crate::paths::iyw_claw_task_artifacts_root(),
        ),
        ("uploads".into(), crate::paths::iyw_claw_uploads_root()),
    ])
}

fn all_roots(data_dir: &Path) -> BTreeMap<String, PathBuf> {
    let mut roots = host_roots(data_dir);
    roots.extend(
        super::external::sources()
            .into_iter()
            .map(|source| (format!("external/{}", source.agent), source.root)),
    );
    roots
}

pub(super) fn capture(data_dir: &Path, work: &Path) -> Result<PathBuf, AppCommandError> {
    let path = work.join(PATHS_ENTRY);
    let bytes = serde_json::to_vec(&BackupPaths {
        version: PATHS_VERSION,
        roots: all_roots(data_dir),
    })
    .map_err(|error| AppCommandError::invalid_input(error.to_string()))?;
    std::fs::write(&path, bytes).map_err(AppCommandError::io)?;
    Ok(path)
}

pub(super) async fn prepare(staging: &Path, data_dir: &Path) -> Result<(), AppCommandError> {
    let mappings = read_mappings(staging, data_dir)?;
    if staging.join(PATHS_ENTRY).is_file() {
        for name in host_roots(data_dir).keys() {
            std::fs::create_dir_all(staging.join(name)).map_err(AppCommandError::io)?;
        }
    }
    super::portable_database::prepare(staging, data_dir, &mappings).await?;
    let root = staging.to_path_buf();
    tokio::task::spawn_blocking(move || super::portable_transcripts::relocate(&root, &mappings))
        .await
        .map_err(|error| {
            AppCommandError::task_execution_failed("Relocate backup paths")
                .with_detail(error.to_string())
        })??;
    Ok(())
}

fn read_mappings(
    staging: &Path,
    data_dir: &Path,
) -> Result<Vec<(String, PathBuf)>, AppCommandError> {
    let path = staging.join(PATHS_ENTRY);
    if !path.exists() {
        return Ok(Vec::new());
    }
    if std::fs::metadata(&path).map_err(AppCommandError::io)?.len() > MAX_PATHS_BYTES {
        return Err(super::unknown_format_error());
    }
    let paths: BackupPaths =
        serde_json::from_slice(&std::fs::read(path).map_err(AppCommandError::io)?)
            .map_err(|_| super::unknown_format_error())?;
    if paths.version != PATHS_VERSION {
        return Err(super::unknown_format_error());
    }
    let targets = all_roots(data_dir);
    let mut mappings = paths
        .roots
        .into_iter()
        .filter_map(|(key, source)| {
            targets
                .get(&key)
                .map(|target| (source.to_string_lossy().into_owned(), target.clone()))
        })
        .collect::<Vec<_>>();
    mappings.sort_by_key(|(source, _)| std::cmp::Reverse(source.len()));
    Ok(mappings)
}

pub(super) fn rebase(value: &str, mappings: &[(String, PathBuf)]) -> Option<String> {
    if value.starts_with("file://") {
        let url = reqwest::Url::parse(value).ok()?;
        if url
            .host_str()
            .is_some_and(|host| !host.is_empty() && host != "localhost")
        {
            return None;
        }
        let decoded = urlencoding::decode(url.path()).ok()?;
        let path = decoded
            .strip_prefix('/')
            .filter(|path| path.as_bytes().get(1) == Some(&b':'))
            .unwrap_or(&decoded);
        let next = rebase_path(path, mappings)?;
        let mut relocated = reqwest::Url::from_file_path(next).ok()?;
        relocated.set_query(url.query());
        relocated.set_fragment(url.fragment());
        return Some(relocated.to_string());
    }
    rebase_path(value, mappings)
}

fn rebase_path(value: &str, mappings: &[(String, PathBuf)]) -> Option<String> {
    let normalized = normalize(value);
    for (source, target) in mappings {
        let source = normalize(source);
        if source.is_empty() || normalized.len() < source.len() {
            continue;
        }
        let Some(prefix) = normalized.get(..source.len()) else {
            continue;
        };
        let windows = source.as_bytes().get(1) == Some(&b':') || source.starts_with("//");
        if !(prefix == source || (windows && prefix.eq_ignore_ascii_case(&source))) {
            continue;
        }
        let suffix = &normalized[source.len()..];
        if !suffix.is_empty() && !suffix.starts_with('/') {
            continue;
        }
        let mut result = target.clone();
        for segment in suffix.split('/').filter(|segment| !segment.is_empty()) {
            if matches!(segment, "." | "..") || segment.contains(':') {
                return None;
            }
            result.push(segment);
        }
        return Some(result.to_string_lossy().into_owned());
    }
    None
}

fn normalize(value: &str) -> String {
    value
        .strip_prefix("\\\\?\\")
        .unwrap_or(value)
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}
