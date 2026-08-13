use std::fs;
use std::io::Read;
use std::path::Path;

const OPENAI_METADATA_PATH: [&str; 2] = ["agents", "openai.yaml"];
const MAX_OPENAI_METADATA_BYTES: u64 = 64 * 1024;
const MAX_SKILL_FRONTMATTER_BYTES: u64 = 64 * 1024;

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
