use std::fs;
use std::path::Path;

use super::{AgentSkillItem, RawLocation, SkillStorageKind};
use crate::acp::types::AgentSkillLayout;

pub(super) fn raw_location(root: &Path, path: &Path, item: AgentSkillItem) -> RawLocation {
    RawLocation {
        enabled: item.enabled,
        item,
        root: root.to_path_buf(),
        canonical_path: fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        agent_types: Vec::new(),
        projection_source: managed_copy_source(path),
    }
}

pub(super) fn identify_skill(
    path: &Path,
    kind: SkillStorageKind,
) -> Option<(String, AgentSkillLayout)> {
    if path.is_dir() && path.join("SKILL.md").is_file() {
        return path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|id| (id.to_string(), AgentSkillLayout::SkillDirectory));
    }
    if matches!(kind, SkillStorageKind::SkillDirectoryOrMarkdownFile)
        && path.is_file()
        && path.extension()?.eq_ignore_ascii_case("md")
    {
        return path
            .file_stem()
            .and_then(|name| name.to_str())
            .map(|id| (id.to_string(), AgentSkillLayout::MarkdownFile));
    }
    None
}

fn managed_copy_source(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path.join(".iyw-claw-managed-copy.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    value.get("source_path")?.as_str().map(ToOwned::to_owned)
}
