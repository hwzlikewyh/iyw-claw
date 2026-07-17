use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

use include_dir::{include_dir, Dir, DirEntry};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{ScienceError, ScienceMetadata};

pub(super) static SCIENCE_BUNDLE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/science");
const SCIENCE_TOML: &str = "science.toml";

#[derive(Debug, Deserialize)]
struct ScienceTomlRoot {
    #[serde(default)]
    skill: Vec<ScienceTomlEntry>,
}

#[derive(Debug, Deserialize)]
struct ScienceTomlEntry {
    id: String,
    category: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    sort_order: i32,
    #[serde(default)]
    featured: bool,
    #[serde(default)]
    accent: Option<String>,
    #[serde(default)]
    needs_key: bool,
    #[serde(default)]
    needs_env: bool,
    #[serde(default)]
    display_name: BTreeMap<String, String>,
    #[serde(default)]
    description: BTreeMap<String, String>,
}

pub(super) fn bundled_metadata() -> &'static [ScienceMetadata] {
    static METADATA: OnceLock<Vec<ScienceMetadata>> = OnceLock::new();
    METADATA.get_or_init(|| match load_metadata() {
        Ok(metadata) => metadata,
        Err(error) => {
            tracing::error!("[Science] failed to load bundled metadata: {error}");
            Vec::new()
        }
    })
}

fn load_metadata() -> Result<Vec<ScienceMetadata>, ScienceError> {
    let file = SCIENCE_BUNDLE
        .get_file(SCIENCE_TOML)
        .ok_or_else(|| ScienceError::Metadata(format!("{SCIENCE_TOML} is missing")))?;
    let source = file
        .contents_utf8()
        .ok_or_else(|| ScienceError::Metadata(format!("{SCIENCE_TOML} is not UTF-8")))?;
    let root: ScienceTomlRoot =
        toml::from_str(source).map_err(|error| ScienceError::Metadata(error.to_string()))?;
    validate_entries(&root.skill)?;

    let mut metadata = root
        .skill
        .into_iter()
        .map(|entry| {
            let bundled_hash = hash_bundled_skill(&entry.id)?;
            Ok(ScienceMetadata {
                id: entry.id,
                category: entry.category,
                icon: entry.icon,
                sort_order: entry.sort_order,
                featured: entry.featured,
                accent: entry.accent,
                needs_key: entry.needs_key,
                needs_env: entry.needs_env,
                display_name: entry.display_name,
                description: entry.description,
                bundled_hash,
            })
        })
        .collect::<Result<Vec<_>, ScienceError>>()?;
    metadata.sort_by(|left, right| {
        left.sort_order
            .cmp(&right.sort_order)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(metadata)
}

fn validate_entries(entries: &[ScienceTomlEntry]) -> Result<(), ScienceError> {
    let mut ids = HashSet::new();
    for entry in entries {
        crate::commands::acp::validate_skill_id(&entry.id)
            .map_err(|error| ScienceError::Metadata(error.to_string()))?;
        if !ids.insert(entry.id.as_str()) {
            return Err(ScienceError::Metadata(format!(
                "duplicate science skill id '{}'",
                entry.id
            )));
        }
        if entry.featured && entry.accent.as_deref().is_none_or(str::is_empty) {
            return Err(ScienceError::Metadata(format!(
                "featured science skill '{}' has no accent",
                entry.id
            )));
        }
        if !entry.display_name.contains_key("en") || !entry.description.contains_key("en") {
            return Err(ScienceError::Metadata(format!(
                "science skill '{}' has no English fallback",
                entry.id
            )));
        }
    }
    Ok(())
}

pub(super) fn find_metadata(skill_id: &str) -> Result<&'static ScienceMetadata, ScienceError> {
    bundled_metadata()
        .iter()
        .find(|metadata| metadata.id == skill_id)
        .ok_or_else(|| ScienceError::NotFound(skill_id.to_string()))
}

pub(super) fn bundled_text(skill_id: &str, relative: &str) -> Option<&'static str> {
    SCIENCE_BUNDLE
        .get_file(format!("skills/{skill_id}/{relative}"))?
        .contents_utf8()
}

pub(super) fn bundled_skill_dir(skill_id: &str) -> Result<&'static Dir<'static>, ScienceError> {
    SCIENCE_BUNDLE
        .get_dir(format!("skills/{skill_id}"))
        .ok_or_else(|| ScienceError::NotFound(skill_id.to_string()))
}

fn hash_bundled_skill(skill_id: &str) -> Result<String, ScienceError> {
    let directory = bundled_skill_dir(skill_id)?;
    let mut files = Vec::new();
    collect_files(directory, &mut files);
    files.sort_by_key(|(path, _)| *path);
    let mut hasher = Sha256::new();
    for (path, contents) in files {
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        hasher.update(contents);
        hasher.update(b"\0");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files<'a>(directory: &'a Dir<'a>, files: &mut Vec<(&'a str, &'a [u8])>) {
    for entry in directory.entries() {
        match entry {
            DirEntry::File(file) => {
                files.push((file.path().to_str().unwrap_or_default(), file.contents()));
            }
            DirEntry::Dir(child) => collect_files(child, files),
        }
    }
}
