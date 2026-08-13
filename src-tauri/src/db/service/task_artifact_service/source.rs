use std::path::{Path, PathBuf};

use reqwest::Url;

const MAX_FILES: usize = 100;
const MAX_SOURCE_CHARS: usize = 4096;
const ARTIFACT_KIND_FILE: &str = "file";
const ARTIFACT_KIND_DIRECTORY: &str = "directory";
pub(super) const ARTIFACT_KIND_URL: &str = "url";

pub(super) struct ResolvedArtifact {
    pub source: String,
    pub path: String,
    pub display_name: String,
    pub kind: String,
}

pub(super) struct CurrentArtifactState {
    pub status: String,
    pub kind: String,
}

fn artifact_kind(metadata: &std::fs::Metadata) -> Result<&'static str, String> {
    match (metadata.is_file(), metadata.is_dir()) {
        (true, _) => Ok(ARTIFACT_KIND_FILE),
        (_, true) => Ok(ARTIFACT_KIND_DIRECTORY),
        _ => Err("unsupported_type".into()),
    }
}

fn validate_source(source: &str) -> Result<(), String> {
    if source.is_empty() {
        return Err("empty_source".into());
    }
    if source.chars().count() > MAX_SOURCE_CHARS || source.contains('\0') {
        return Err("invalid_source".into());
    }
    Ok(())
}

fn resolve_url(source: &str) -> Option<Result<ResolvedArtifact, String>> {
    let parsed = Url::parse(source);
    let looks_like_url = source
        .split_once("://")
        .is_some_and(|(scheme, _)| is_url_scheme(scheme));
    let url = match parsed {
        Ok(url) if matches!(url.scheme(), "http" | "https") => url,
        Ok(_) if looks_like_url => return Some(Err("unsupported_url_scheme".into())),
        Err(_) if looks_like_url => return Some(Err("invalid_url".into())),
        _ => return None,
    };
    if url.host_str().is_none() {
        return Some(Err("invalid_url".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Some(Err("url_credentials_not_allowed".into()));
    }
    let display_name = url
        .path_segments()
        .and_then(|segments| segments.filter(|part| !part.is_empty()).next_back())
        .or_else(|| url.host_str())
        .unwrap_or(source)
        .to_string();
    Some(Ok(ResolvedArtifact {
        source: source.to_string(),
        path: url.to_string(),
        display_name,
        kind: ARTIFACT_KIND_URL.into(),
    }))
}

fn is_safe_url(source: &str) -> bool {
    Url::parse(source).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

fn is_url_scheme(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

fn resolve_path(working_dir: &Path, source: &str) -> Result<ResolvedArtifact, String> {
    if is_windows_device_path(source) {
        return Err("invalid_path".into());
    }
    let candidate = PathBuf::from(source);
    let is_relative = !candidate.is_absolute();
    let joined = if is_relative {
        working_dir.join(candidate)
    } else {
        candidate
    };
    let metadata = std::fs::metadata(&joined).map_err(map_metadata_error)?;
    let kind = artifact_kind(&metadata)?.to_string();
    let canonical = std::fs::canonicalize(&joined).map_err(|_| "inaccessible".to_string())?;
    ensure_relative_path_stays_in_root(working_dir, &canonical, is_relative)?;
    let path = normalize_canonical_path(canonical);
    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(source)
        .to_string();
    Ok(ResolvedArtifact {
        source: source.to_string(),
        path: path.to_string_lossy().into_owned(),
        display_name,
        kind,
    })
}

fn map_metadata_error(error: std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "missing".into(),
        _ => "inaccessible".into(),
    }
}

fn ensure_relative_path_stays_in_root(
    working_dir: &Path,
    canonical: &Path,
    is_relative: bool,
) -> Result<(), String> {
    if !is_relative {
        return Ok(());
    }
    let root = std::fs::canonicalize(working_dir).map_err(|_| "inaccessible".to_string())?;
    if canonical.starts_with(root) {
        return Ok(());
    }
    Err("path_escape".into())
}

pub(super) fn resolve_sources(
    working_dir: &Path,
    sources: Vec<String>,
) -> (Vec<ResolvedArtifact>, Vec<(String, String)>) {
    let mut resolved = Vec::new();
    let mut rejected = Vec::new();
    for (index, original) in sources.into_iter().enumerate() {
        let source = original.trim();
        let result = if index >= MAX_FILES {
            Err("too_many_artifacts".into())
        } else {
            validate_source(source).and_then(|()| match resolve_url(source) {
                Some(result) => result,
                None => resolve_path(working_dir, source),
            })
        };
        match result {
            Ok(artifact) => resolved.push(artifact),
            Err(reason) => rejected.push((original, reason)),
        }
    }
    (resolved, rejected)
}

pub(super) fn current_artifact_state(path: &str, stored_kind: &str) -> CurrentArtifactState {
    if stored_kind == ARTIFACT_KIND_URL {
        return CurrentArtifactState {
            status: if is_safe_url(path) {
                "available".into()
            } else {
                "inaccessible".into()
            },
            kind: ARTIFACT_KIND_URL.into(),
        };
    }
    let (status, kind) = match std::fs::metadata(path) {
        Ok(metadata) => match artifact_kind(&metadata) {
            Ok(kind) => ("available", kind),
            Err(_) => ("inaccessible", stored_kind),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ("missing", stored_kind),
        Err(_) => ("inaccessible", stored_kind),
    };
    CurrentArtifactState {
        status: status.into(),
        kind: kind.into(),
    }
}

#[cfg(windows)]
fn is_windows_device_path(path: &str) -> bool {
    let normalized = path.replace('/', "\\").to_ascii_lowercase();
    normalized.starts_with(r"\\.\") || normalized.starts_with(r"\\?\globalroot\")
}

#[cfg(not(windows))]
fn is_windows_device_path(_path: &str) -> bool {
    false
}

#[cfg(windows)]
fn normalize_canonical_path(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = value.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    drop(value);
    path
}

#[cfg(not(windows))]
fn normalize_canonical_path(path: PathBuf) -> PathBuf {
    path
}
