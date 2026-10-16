use std::fs;
use std::io::{Read, Write};
use std::path::Path;

const OPENAI_METADATA_PATH: [&str; 2] = ["agents", "openai.yaml"];
const MAX_OPENAI_METADATA_BYTES: u64 = 64 * 1024;
const MAX_SKILL_FRONTMATTER_BYTES: u64 = 64 * 1024;
const MAX_LEGACY_SKILL_BYTES: u64 = 1024 * 1024;

pub(super) fn repair_market_skill_description(skill_id: &str, source: &Path) {
    match repair_legacy_skill_description(&source.join("SKILL.md")) {
        Ok(true) => tracing::info!(
            skill_id,
            "[skills] repaired legacy description; original package hash retained"
        ),
        Ok(false) => {}
        Err(error) => tracing::warn!(skill_id, error = %error,
            "[skills] legacy description repair failed; continuing reconciliation"),
    }
}

pub(super) fn repair_legacy_skill_description(content_path: &Path) -> Result<bool, String> {
    let content = read_text(content_path, MAX_LEGACY_SKILL_BYTES)
        .ok_or_else(|| "failed to read legacy Skill entry within size limit".to_string())?;
    let Some(repaired) = legacy_skill_description(&content) else {
        return Ok(false);
    };
    let parent = content_path.parent().ok_or("Skill entry has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("failed to stage legacy Skill repair: {error}"))?;
    temporary
        .write_all(repaired.as_bytes())
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("failed to write legacy Skill repair: {error}"))?;
    temporary
        .persist(content_path)
        .map_err(|error| format!("failed to publish legacy Skill repair: {}", error.error))?;
    Ok(true)
}

fn legacy_skill_description(content: &str) -> Option<String> {
    let frontmatter = crate::acp::skill_routing::read_frontmatter(content)?;
    if frontmatter.get("description").is_some() {
        return None;
    }
    let description = [
        frontmatter.get("summary"),
        frontmatter.get("short-description"),
        frontmatter
            .get("metadata")
            .and_then(|value| value.get("short-description")),
    ]
    .into_iter()
    .flatten()
    .filter_map(serde_yaml::Value::as_str)
    .find(|value| !value.trim().is_empty())?;
    let description = serde_json::to_string(description.trim()).ok()?;
    let (opening, rest) = content.split_once('\n')?;
    let newline = if opening.ends_with('\r') {
        "\r\n"
    } else {
        "\n"
    };
    Some(format!(
        "{opening}\ndescription: {description}{newline}{rest}"
    ))
}

pub(super) fn read_skill_display_name(skill_path: &Path, content_path: &Path) -> Option<String> {
    read_openai_display_name(skill_path).or_else(|| read_frontmatter_display_name(content_path))
}

fn read_openai_display_name(skill_path: &Path) -> Option<String> {
    if !skill_path.is_dir() {
        return None;
    }
    let path = OPENAI_METADATA_PATH
        .iter()
        .fold(skill_path.to_path_buf(), |path, segment| path.join(segment));
    let yaml = read_yaml(&path, MAX_OPENAI_METADATA_BYTES)?;
    yaml.get("interface")?
        .get("display_name")?
        .as_str()
        .and_then(valid_display_name)
}

fn read_frontmatter_display_name(content_path: &Path) -> Option<String> {
    let raw = read_text_head(content_path, MAX_SKILL_FRONTMATTER_BYTES)?;
    let mut lines = raw.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let frontmatter = lines
        .take_while(|line| !matches!(line.trim(), "---" | "..."))
        .collect::<Vec<_>>()
        .join("\n");
    let yaml: serde_yaml::Value = serde_yaml::from_str(&frontmatter).ok()?;
    yaml.get("displayName")
        .or_else(|| yaml.get("display_name"))?
        .as_str()
        .and_then(valid_display_name)
}

fn read_yaml(path: &Path, max_bytes: u64) -> Option<serde_yaml::Value> {
    let raw = read_text(path, max_bytes)?;
    serde_yaml::from_str(&raw).ok()
}

fn read_text(path: &Path, max_bytes: u64) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn read_text_head(path: &Path, max_bytes: u64) -> Option<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(max_bytes)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn valid_display_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.contains('\r') || trimmed.contains('\n') {
        None
    } else {
        Some(trimmed.to_string())
    }
}
