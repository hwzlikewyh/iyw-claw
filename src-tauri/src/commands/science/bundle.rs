use std::collections::{BTreeMap, HashSet};
use std::fs;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::filesystem::{extract_skill, hash_disk_directory};
use super::metadata::{bundled_metadata, bundled_text, find_metadata};
use super::{central_path, mutation_lock, ScienceError, ScienceInstallReport, ScienceListItem};

const MANIFEST_FILE: &str = ".manifest.science.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    iyw_claw_version: String,
    #[serde(default)]
    installed_at: String,
    #[serde(default)]
    science: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ManifestEntry {
    #[serde(default)]
    hash: String,
    #[serde(default)]
    installed_at: String,
    #[serde(default)]
    pending_user_review: bool,
}

enum InstallAction {
    Skipped,
    Installed,
    Updated,
    BackedUp,
}

fn manifest_path() -> std::path::PathBuf {
    crate::commands::experts::central_experts_dir().join(MANIFEST_FILE)
}

fn load_manifest() -> Manifest {
    fs::read_to_string(manifest_path())
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_manifest(manifest: &Manifest) -> Result<(), ScienceError> {
    let path = manifest_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(manifest)
        .map_err(|error| ScienceError::Metadata(error.to_string()))?;
    fs::write(path, content)?;
    Ok(())
}

pub(super) async fn ensure_installed() -> ScienceInstallReport {
    let _guard = mutation_lock().lock().await;
    tokio::task::spawn_blocking(ensure_installed_blocking)
        .await
        .unwrap_or_else(|error| ScienceInstallReport {
            errors: vec![format!("science installer join error: {error}")],
            ..ScienceInstallReport::default()
        })
}

fn ensure_installed_blocking() -> ScienceInstallReport {
    let mut report = ScienceInstallReport::default();
    if let Err(error) = validate_disjoint_ids() {
        report.errors.push(error.to_string());
        return report;
    }
    if let Err(error) = fs::create_dir_all(crate::commands::experts::central_experts_dir()) {
        report
            .errors
            .push(format!("create central skills directory: {error}"));
        return report;
    }

    let mut manifest = load_manifest();
    for metadata in bundled_metadata() {
        match install_or_refresh(metadata, &mut manifest) {
            Ok(InstallAction::Skipped) => {}
            Ok(InstallAction::Installed) => report.installed_count += 1,
            Ok(InstallAction::Updated) => report.updated_count += 1,
            Ok(InstallAction::BackedUp) => {
                report.updated_count += 1;
                report.pending_user_review.push(metadata.id.clone());
            }
            Err(error) => report.errors.push(format!("{}: {error}", metadata.id)),
        }
    }
    manifest.iyw_claw_version = env!("CARGO_PKG_VERSION").to_string();
    manifest.installed_at = Utc::now().to_rfc3339();
    if let Err(error) = save_manifest(&manifest) {
        report
            .errors
            .push(format!("save science manifest: {error}"));
    }
    report
}

fn validate_disjoint_ids() -> Result<(), ScienceError> {
    let mut other_ids = HashSet::new();
    other_ids.extend(crate::commands::experts::managed_expert_ids());
    other_ids.extend(crate::commands::office_tools::managed_office_skill_ids());
    other_ids.extend(crate::commands::internet_tools::managed_internet_skill_ids());
    if let Some(id) = bundled_metadata()
        .iter()
        .map(|metadata| metadata.id.as_str())
        .find(|id| other_ids.contains(*id))
    {
        return Err(ScienceError::IdCollision(id.to_string()));
    }
    Ok(())
}

fn install_or_refresh(
    metadata: &super::ScienceMetadata,
    manifest: &mut Manifest,
) -> Result<InstallAction, ScienceError> {
    let path = central_path(&metadata.id);
    let previous = manifest
        .science
        .get(&metadata.id)
        .cloned()
        .unwrap_or_default();
    if !path.exists() {
        extract_skill(metadata, &path)?;
        update_manifest_entry(manifest, metadata, false);
        return Ok(InstallAction::Installed);
    }

    let disk_hash = hash_disk_directory(&path).unwrap_or_default();
    if disk_hash == metadata.bundled_hash {
        if previous.hash != metadata.bundled_hash {
            update_manifest_entry(manifest, metadata, false);
        }
        return Ok(InstallAction::Skipped);
    }

    let user_modified = previous.hash.is_empty() || disk_hash != previous.hash;
    if user_modified {
        let backup = crate::commands::experts::central_experts_dir().join(format!(
            "{}.user-backup-{}",
            metadata.id,
            Utc::now().format("%Y%m%d-%H%M%S")
        ));
        fs::rename(&path, backup)?;
        extract_skill(metadata, &path)?;
        update_manifest_entry(manifest, metadata, true);
        return Ok(InstallAction::BackedUp);
    }

    crate::commands::acp::remove_skill_entry(&path)
        .map_err(|error| ScienceError::Io(error.to_string()))?;
    extract_skill(metadata, &path)?;
    update_manifest_entry(manifest, metadata, false);
    Ok(InstallAction::Updated)
}

fn update_manifest_entry(
    manifest: &mut Manifest,
    metadata: &super::ScienceMetadata,
    pending_user_review: bool,
) {
    manifest.science.insert(
        metadata.id.clone(),
        ManifestEntry {
            hash: metadata.bundled_hash.clone(),
            installed_at: Utc::now().to_rfc3339(),
            pending_user_review,
        },
    );
}

pub(super) fn list() -> Result<Vec<ScienceListItem>, ScienceError> {
    let manifest = load_manifest();
    Ok(bundled_metadata()
        .iter()
        .cloned()
        .map(|metadata| {
            let path = central_path(&metadata.id);
            ScienceListItem {
                installed_centrally: path.exists(),
                user_modified: manifest
                    .science
                    .get(&metadata.id)
                    .is_some_and(|entry| entry.pending_user_review),
                central_path: path.to_string_lossy().to_string(),
                metadata,
            }
        })
        .collect())
}

pub(super) fn read_content(skill_id: &str) -> Result<String, ScienceError> {
    let skill_id = crate::commands::acp::validate_skill_id(skill_id)
        .map_err(|error| ScienceError::Metadata(error.to_string()))?;
    find_metadata(&skill_id)?;
    let path = central_path(&skill_id).join("SKILL.md");
    if path.exists() {
        return Ok(fs::read_to_string(path)?);
    }
    bundled_text(&skill_id, "SKILL.md")
        .map(str::to_string)
        .ok_or(ScienceError::CentralUnavailable(skill_id))
}

#[cfg(test)]
pub(super) fn catalog_ids_are_disjoint() -> bool {
    validate_disjoint_ids().is_ok()
}
