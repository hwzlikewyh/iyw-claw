use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::acp::skill_routing::original_legacy_skill_description_content;
use crate::acp::types::AgentSkillLayout;
use crate::commands::experts::RUNTIME_ENV_DIR_NAMES;

mod cache;

const MAX_HASH_FILES: usize = 1_024;
const MAX_HASH_BYTES: u64 = 50 * 1024 * 1024;
const INTERNAL_MARKERS: [&str; 4] = [
    ".iyw-claw-market-skill.json",
    ".iyw-claw-official-skill.json",
    ".iyw-claw-publish-state.json",
    ".iyw-claw-managed-copy.json",
];

struct TreeFile {
    path: String,
    size: usize,
    sha256: String,
}

struct TreeEntry {
    absolute: PathBuf,
    relative: PathBuf,
    length: u64,
    modified: Option<SystemTime>,
}

impl TreeEntry {
    fn unchanged(&self) -> bool {
        fs::metadata(&self.absolute).is_ok_and(|metadata| {
            metadata.len() == self.length && metadata.modified().ok() == self.modified
        })
    }

    fn new(absolute: &Path, relative: &Path, metadata: &fs::Metadata) -> Self {
        Self {
            absolute: absolute.to_path_buf(),
            relative: relative.to_path_buf(),
            length: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}

pub(crate) fn hash_skill_path(layout: AgentSkillLayout, path: &Path) -> Result<String, String> {
    let mut files = match layout {
        AgentSkillLayout::MarkdownFile => vec![hash_file(path, Path::new("SKILL.md"))?],
        AgentSkillLayout::SkillDirectory => collect_tree_files(path)?,
    };
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(hash_file_tree(&files))
}

pub(crate) fn cached_skill_inventory_hash(
    layout: AgentSkillLayout,
    path: &Path,
) -> Result<String, String> {
    let entries = match layout {
        AgentSkillLayout::MarkdownFile => vec![markdown_entry(path)?],
        AgentSkillLayout::SkillDirectory => list_tree_entries(path)?,
    };
    // 文件元数据未变时复用上次结果，避免每次清单扫描都重读整棵技能目录。
    let fingerprint = cache::fingerprint(&entries);
    if let Some(hash) = fingerprint
        .as_deref()
        .and_then(|value| cache::lookup(path, value))
    {
        return Ok(hash);
    }
    let mut files = hash_entries(&entries)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let hash = hash_file_tree(&files);
    if let Some(fingerprint) = fingerprint.filter(|_| entries.iter().all(TreeEntry::unchanged)) {
        cache::store(path, fingerprint, &hash);
    }
    Ok(hash)
}

pub(crate) fn hash_skill_path_before_description_repair(
    layout: AgentSkillLayout,
    path: &Path,
) -> Option<String> {
    let content_path = match layout {
        AgentSkillLayout::MarkdownFile => path.to_path_buf(),
        AgentSkillLayout::SkillDirectory => path.join("SKILL.md"),
    };
    if fs::metadata(&content_path).ok()?.len() > MAX_HASH_BYTES {
        return None;
    }
    let content = fs::read_to_string(&content_path).ok()?;
    let original = original_legacy_skill_description_content(&content)?;
    let mut files = match layout {
        AgentSkillLayout::MarkdownFile => vec![hash_file(path, Path::new("SKILL.md")).ok()?],
        AgentSkillLayout::SkillDirectory => collect_tree_files(path).ok()?,
    };
    let entry = files.iter_mut().find(|file| file.path == "SKILL.md")?;
    if entry.sha256 != format!("{:x}", Sha256::digest(content.as_bytes())) {
        return None;
    }
    entry.size = original.len();
    entry.sha256 = format!("{:x}", Sha256::digest(original.as_bytes()));
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Some(hash_file_tree(&files))
}

fn collect_tree_files(root: &Path) -> Result<Vec<TreeFile>, String> {
    hash_entries(&list_tree_entries(root)?)
}

fn hash_entries(entries: &[TreeEntry]) -> Result<Vec<TreeFile>, String> {
    entries
        .iter()
        .map(|entry| hash_file(&entry.absolute, &entry.relative))
        .collect()
}

fn markdown_entry(path: &Path) -> Result<TreeEntry, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    Ok(TreeEntry::new(path, Path::new("SKILL.md"), &metadata))
}

fn list_tree_entries(root: &Path) -> Result<Vec<TreeEntry>, String> {
    let mut files = Vec::new();
    let mut total_bytes = 0_u64;
    let entries = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() != 1
                || !entry.file_type().is_dir()
                || !RUNTIME_ENV_DIR_NAMES
                    .iter()
                    .any(|name| entry.file_name().eq_ignore_ascii_case(name))
        });
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| error.to_string())?;
        if is_ignored_path(relative) {
            continue;
        }
        if files.len() >= MAX_HASH_FILES {
            return Err(format!("Skill contains more than {MAX_HASH_FILES} files"));
        }
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        total_bytes = total_bytes.saturating_add(metadata.len());
        if total_bytes > MAX_HASH_BYTES {
            return Err("Skill content exceeds the 50 MiB hashing limit".to_string());
        }
        files.push(TreeEntry::new(entry.path(), relative, &metadata));
    }
    // 固定顺序，保证同一目录的指纹稳定。
    files.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(files)
}

fn is_ignored_path(relative: &Path) -> bool {
    if RUNTIME_ENV_DIR_NAMES.iter().any(|name| {
        relative
            .components()
            .next()
            .is_some_and(|component| component.as_os_str().eq_ignore_ascii_case(name))
    }) {
        return true;
    }
    relative
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            INTERNAL_MARKERS
                .iter()
                .any(|marker| name.eq_ignore_ascii_case(marker))
        })
}

fn hash_file(path: &Path, relative: &Path) -> Result<TreeFile, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    Ok(TreeFile {
        path: normalize_path(relative),
        size: bytes.len(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
    })
}

fn hash_file_tree(files: &[TreeFile]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update(b"\0");
        hasher.update(file.size.to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(file.sha256.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
