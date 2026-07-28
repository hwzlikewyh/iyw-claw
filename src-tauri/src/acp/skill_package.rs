use std::collections::{BTreeSet, HashSet};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::app_error::AppCommandError;

const MAX_FILES: usize = 512;
const MAX_ARCHIVE_ENTRIES: usize = 1024;
const MAX_EXPANDED_BYTES: u64 = 50 * 1024 * 1024;
const MAX_SKILL_MD_BYTES: u64 = 1024 * 1024;
const MAX_PATH_BYTES: usize = 512;
const MARKET_MARKER: &str = ".iyw-claw-market-skill.json";
const OFFICIAL_MARKER: &str = ".iyw-claw-official-skill.json";
const PUBLISH_STATE_MARKER: &str = ".iyw-claw-publish-state.json";
const MANAGED_COPY_MARKER: &str = ".iyw-claw-managed-copy.json";

pub struct PackageFile {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    sha256: String,
}

pub struct ValidatedSkillPackage {
    pub files: Vec<PackageFile>,
    pub content_sha256: String,
}

pub fn validate_zip(
    bytes: &[u8],
    expected_content_sha256: &str,
) -> Result<ValidatedSkillPackage, AppCommandError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(invalid_zip)?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(invalid_package("Skill archive contains too many entries"));
    }
    let mut files = Vec::new();
    let mut seen_files = BTreeSet::new();
    let mut seen_directories = HashSet::new();
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(invalid_zip)?;
        if entry.encrypted() || entry.is_symlink() {
            return Err(invalid_package("Encrypted files and links are not allowed"));
        }
        if entry.is_dir() {
            continue;
        }
        if !entry.is_file() || files.len() >= MAX_FILES {
            return Err(invalid_package("Skill archive contains too many files"));
        }
        let path = validate_path(entry.name())?;
        register_path(&path, &mut seen_files, &mut seen_directories)?;
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_EXPANDED_BYTES {
            return Err(invalid_package("Expanded Skill exceeds the 50 MiB limit"));
        }
        let mut content = Vec::with_capacity(entry.size().min(usize::MAX as u64) as usize);
        entry
            .by_ref()
            .take(MAX_EXPANDED_BYTES + 1)
            .read_to_end(&mut content)
            .map_err(|error| invalid_zip(error.to_string()))?;
        if content.len() as u64 != entry.size() {
            return Err(invalid_package("Skill archive entry size is invalid"));
        }
        let sha256 = hash_bytes(&content);
        files.push(PackageFile {
            path,
            bytes: content,
            sha256,
        });
    }
    validate_skill_entry(&files)?;
    files.sort_by(|left, right| normalized_path(&left.path).cmp(&normalized_path(&right.path)));
    let content_sha256 = hash_file_tree(&files);
    if !content_sha256.eq_ignore_ascii_case(expected_content_sha256.trim()) {
        return Err(invalid_package("Skill file tree integrity check failed"));
    }
    Ok(ValidatedSkillPackage {
        files,
        content_sha256,
    })
}

fn validate_path(raw: &str) -> Result<PathBuf, AppCommandError> {
    let parts: Vec<&str> = raw.split('/').collect();
    if raw.is_empty()
        || raw.as_bytes().len() > MAX_PATH_BYTES
        || raw.contains('\0')
        || raw.contains('\\')
        || raw.starts_with('/')
        || parts.iter().any(|part| !is_safe_path_segment(part))
        || is_internal_marker(raw)
    {
        return Err(invalid_package("Skill archive contains an unsafe path"));
    }
    let path = PathBuf::from(raw);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid_package("Skill archive contains an unsafe path"));
    }
    Ok(path)
}

fn is_safe_path_segment(part: &str) -> bool {
    if part.is_empty()
        || part == "."
        || part == ".."
        || part.ends_with('.')
        || part.ends_with(' ')
        || part
            .chars()
            .any(|value| value < ' ' || matches!(value, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return false;
    }
    let stem = part
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !is_reserved_numbered_device(&stem, "COM")
        && !is_reserved_numbered_device(&stem, "LPT")
}

fn is_reserved_numbered_device(value: &str, prefix: &str) -> bool {
    matches!(
        value.strip_prefix(prefix),
        Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
    )
}

fn is_internal_marker(raw: &str) -> bool {
    [
        MARKET_MARKER,
        OFFICIAL_MARKER,
        PUBLISH_STATE_MARKER,
        MANAGED_COPY_MARKER,
    ]
    .into_iter()
    .any(|marker| raw.eq_ignore_ascii_case(marker))
}

fn register_path(
    path: &Path,
    files: &mut BTreeSet<String>,
    directories: &mut HashSet<String>,
) -> Result<(), AppCommandError> {
    let key = normalized_path(path).to_ascii_lowercase();
    if files.contains(&key) || directories.contains(&key) {
        return Err(invalid_package("Skill archive contains conflicting paths"));
    }
    let parts: Vec<&str> = key.split('/').collect();
    for index in 1..parts.len() {
        let directory = parts[..index].join("/");
        if files.contains(&directory) {
            return Err(invalid_package("Skill archive contains conflicting paths"));
        }
        directories.insert(directory);
    }
    files.insert(key);
    Ok(())
}

fn validate_skill_entry(files: &[PackageFile]) -> Result<(), AppCommandError> {
    let entry = files
        .iter()
        .find(|file| file.path == Path::new("SKILL.md"))
        .ok_or_else(|| invalid_package("Skill archive root must contain SKILL.md"))?;
    if entry.bytes.is_empty() || entry.bytes.len() as u64 > MAX_SKILL_MD_BYTES {
        return Err(invalid_package("SKILL.md is empty or too large"));
    }
    let content = std::str::from_utf8(&entry.bytes)
        .map_err(|_| invalid_package("SKILL.md must be UTF-8 text"))?;
    if content.trim().is_empty() {
        return Err(invalid_package("SKILL.md must not be blank"));
    }
    Ok(())
}

fn hash_file_tree(files: &[PackageFile]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        let path = normalized_path(&file.path);
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        hasher.update(file.bytes.len().to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(file.sha256.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn invalid_zip(error: impl ToString) -> AppCommandError {
    invalid_package("Skill archive is not a valid ZIP").with_detail(error.to_string())
}

fn invalid_package(message: impl Into<String>) -> AppCommandError {
    AppCommandError::invalid_input(message)
}
