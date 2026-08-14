use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::acp::types::AgentSkillLayout;
use crate::commands::experts::RUNTIME_ENV_DIR_NAMES;

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

pub(crate) fn hash_skill_path(layout: AgentSkillLayout, path: &Path) -> Result<String, String> {
    let mut files = match layout {
        AgentSkillLayout::MarkdownFile => vec![hash_file(path, Path::new("SKILL.md"))?],
        AgentSkillLayout::SkillDirectory => collect_tree_files(path)?,
    };
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(hash_file_tree(&files))
}

fn collect_tree_files(root: &Path) -> Result<Vec<TreeFile>, String> {
    let mut files = Vec::new();
    let mut total_bytes = 0_u64;
    for entry in WalkDir::new(root).follow_links(false) {
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
        total_bytes =
            total_bytes.saturating_add(entry.metadata().map_err(|e| e.to_string())?.len());
        if total_bytes > MAX_HASH_BYTES {
            return Err("Skill content exceeds the 50 MiB hashing limit".to_string());
        }
        files.push(hash_file(entry.path(), relative)?);
    }
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
