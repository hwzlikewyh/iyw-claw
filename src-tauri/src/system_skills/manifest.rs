use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use semver::Version;
use serde::Deserialize;

use crate::app_error::AppCommandError;

const SUPPORTED_SCHEMA_VERSION: u32 = 1;
const EMBEDDED_MANIFEST: &str = include_str!("../../experts/skills/experts.toml");

#[derive(Debug, Deserialize)]
struct RemoteManifest {
    bundle: BundleMetadata,
    #[serde(default)]
    expert: Vec<RemoteExpert>,
}

#[derive(Debug, Deserialize)]
struct BundleMetadata {
    schema_version: u32,
    version: String,
    min_app_version: String,
}

#[derive(Debug, Deserialize)]
struct RemoteExpert {
    id: String,
    #[serde(default)]
    dependencies: Vec<String>,
}

pub fn embedded_version() -> Result<Version, AppCommandError> {
    let manifest: RemoteManifest = toml::from_str(EMBEDDED_MANIFEST).map_err(|error| {
        AppCommandError::configuration_invalid("Embedded system skill manifest is invalid")
            .with_detail(error.to_string())
    })?;
    parse_version(&manifest.bundle.version, "embedded system skill version")
}

pub fn validate_checkout(root: &Path, tag: &str) -> Result<Vec<String>, AppCommandError> {
    let manifest_path = root.join("experts.toml");
    if !manifest_path.is_file() {
        return Err(AppCommandError::configuration_invalid(
            "System skill release is missing experts.toml",
        ));
    }
    let ids = validate_manifest(&manifest_path, tag)?;
    validate_skill_paths(root, ids)
}

fn validate_manifest(path: &Path, tag: &str) -> Result<Vec<String>, AppCommandError> {
    let content = std::fs::read_to_string(path).map_err(AppCommandError::io)?;
    let manifest: RemoteManifest = toml::from_str(&content).map_err(|error| {
        AppCommandError::configuration_invalid("System skill manifest is invalid")
            .with_detail(error.to_string())
    })?;
    if manifest.bundle.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(AppCommandError::configuration_invalid(format!(
            "Unsupported system skill schema {}",
            manifest.bundle.schema_version
        )));
    }
    if tag != format!("v{}", manifest.bundle.version) {
        return Err(AppCommandError::configuration_invalid(
            "System skill tag does not match manifest version",
        ));
    }
    let minimum = parse_version(&manifest.bundle.min_app_version, "minimum app version")?;
    let current = parse_version(env!("CARGO_PKG_VERSION"), "current app version")?;
    if current < minimum {
        return Err(AppCommandError::configuration_invalid(format!(
            "System skills {tag} require iyw-claw {minimum} or newer"
        )));
    }
    validate_dependency_graph(&manifest.expert)?;
    Ok(manifest
        .expert
        .into_iter()
        .map(|expert| expert.id)
        .collect())
}

fn validate_dependency_graph(experts: &[RemoteExpert]) -> Result<(), AppCommandError> {
    let ids = experts
        .iter()
        .map(|expert| expert.id.as_str())
        .collect::<BTreeSet<_>>();
    if ids.len() != experts.len() {
        return Err(AppCommandError::configuration_invalid(
            "System skill manifest contains duplicate ids",
        ));
    }
    for expert in experts {
        let mut unique = BTreeSet::new();
        for dependency in &expert.dependencies {
            crate::commands::acp::validate_skill_id(dependency).map_err(|error| {
                AppCommandError::configuration_invalid(format!(
                    "Invalid dependency for system skill '{}': {error}",
                    expert.id
                ))
            })?;
            if dependency == &expert.id {
                return Err(AppCommandError::configuration_invalid(format!(
                    "System skill '{}' depends on itself",
                    expert.id
                )));
            }
            if !unique.insert(dependency.as_str()) {
                return Err(AppCommandError::configuration_invalid(format!(
                    "System skill '{}' repeats dependency '{dependency}'",
                    expert.id
                )));
            }
            if !ids.contains(dependency.as_str()) {
                return Err(AppCommandError::configuration_invalid(format!(
                    "System skill '{}' requires missing dependency '{dependency}'",
                    expert.id
                )));
            }
        }
    }
    let graph = experts
        .iter()
        .map(|expert| (expert.id.as_str(), expert.dependencies.as_slice()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for id in graph.keys() {
        visit_dependency(id, &graph, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_dependency<'a>(
    id: &'a str,
    graph: &std::collections::BTreeMap<&'a str, &'a [String]>,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> Result<(), AppCommandError> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return Err(AppCommandError::configuration_invalid(format!(
            "System skill dependency cycle detected at '{id}'"
        )));
    }
    for dependency in graph.get(id).into_iter().flat_map(|values| values.iter()) {
        visit_dependency(dependency.as_str(), graph, visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id);
    Ok(())
}

fn validate_skill_paths(root: &Path, ids: Vec<String>) -> Result<Vec<String>, AppCommandError> {
    if ids.is_empty() {
        return Err(AppCommandError::configuration_invalid(
            "System skill manifest contains no skills",
        ));
    }
    let canonical_root = canonical(root)?;
    let mut unique = BTreeSet::new();
    for id in &ids {
        crate::commands::acp::validate_skill_id(id).map_err(|error| {
            AppCommandError::configuration_invalid(format!("Invalid system skill id: {error}"))
        })?;
        if !unique.insert(id.clone()) {
            return Err(AppCommandError::configuration_invalid(format!(
                "Duplicate system skill id: {id}"
            )));
        }
        let skill_dir = canonical(&root.join(id))?;
        if !skill_dir.starts_with(&canonical_root) || !skill_dir.join("SKILL.md").is_file() {
            return Err(AppCommandError::configuration_invalid(format!(
                "System skill {id} is missing SKILL.md"
            )));
        }
    }
    Ok(ids)
}

fn parse_version(value: &str, label: &str) -> Result<Version, AppCommandError> {
    Version::parse(value).map_err(|error| {
        AppCommandError::configuration_invalid(format!("Invalid {label}"))
            .with_detail(error.to_string())
    })
}

fn canonical(path: &Path) -> Result<PathBuf, AppCommandError> {
    std::fs::canonicalize(path).map_err(AppCommandError::io)
}
