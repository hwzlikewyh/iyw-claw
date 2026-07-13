use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::profile_import::{ProfileImportError, ProfileImportReport, ProfileImportSpec};
use crate::acp::registry;

use super::profile_import_io::{
    canonicalize, create_dir, io_error, is_link_or_reparse_point, read_dir,
};

pub(super) struct PreparedProfile {
    pub(super) key: &'static str,
    pub(super) stage: PathBuf,
    pub(super) destination: PathBuf,
    pub(super) report: ProfileImportReport,
}

pub(super) fn import_profile_specs(
    paths: &AgentStoragePaths,
    specs: &[ProfileImportSpec],
) -> Result<ProfileImportReport, ProfileImportError> {
    validate_specs(paths, specs)?;
    let operation = paths
        .staging_dir()
        .join(format!("profile-import-{}", uuid::Uuid::new_v4()));
    create_dir(&operation)?;
    let prepared = match prepare_profiles(&operation, specs) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = fs::remove_dir_all(&operation);
            return Err(error);
        }
    };
    super::profile_import_activation::activate_profiles(&operation, prepared)
}

fn validate_specs(
    paths: &AgentStoragePaths,
    specs: &[ProfileImportSpec],
) -> Result<(), ProfileImportError> {
    let destinations: HashSet<PathBuf> = registry::all_acp_agents()
        .into_iter()
        .map(|agent| paths.profile(agent).root)
        .collect();
    for spec in specs {
        if !destinations.contains(&spec.destination_root) {
            return Err(ProfileImportError::DestinationOutsideStorage(
                spec.destination_root.clone(),
            ));
        }
        for entry in &spec.entries {
            validate_relative(&entry.source_relative)?;
            validate_relative(&entry.destination_relative)?;
        }
    }
    Ok(())
}

fn validate_relative(path: &Path) -> Result<(), ProfileImportError> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(ProfileImportError::UnsafePath(path.to_path_buf()));
    }
    Ok(())
}

fn prepare_profiles(
    operation: &Path,
    specs: &[ProfileImportSpec],
) -> Result<Vec<PreparedProfile>, ProfileImportError> {
    let mut prepared = Vec::new();
    for spec in specs {
        if let Some(profile) = prepare_profile(operation, spec)? {
            prepared.push(profile);
        }
    }
    Ok(prepared)
}

fn prepare_profile(
    operation: &Path,
    spec: &ProfileImportSpec,
) -> Result<Option<PreparedProfile>, ProfileImportError> {
    let key = registry::registry_id_for(spec.agent_type);
    let stage = operation.join("prepared").join(key);
    create_dir(&stage)?;
    if spec.destination_root.exists() {
        copy_existing_tree(&spec.destination_root, &stage)?;
    }
    let mut report = ProfileImportReport::default();
    for entry in &spec.entries {
        copy_import_entry(entry, &stage, &mut report)?;
    }
    if report.imported_files == 0 {
        let _ = fs::remove_dir_all(&stage);
        return Ok(None);
    }
    Ok(Some(PreparedProfile {
        key,
        stage,
        destination: spec.destination_root.clone(),
        report,
    }))
}

fn copy_import_entry(
    entry: &crate::acp::profile_import::ProfileImportEntry,
    stage: &Path,
    report: &mut ProfileImportReport,
) -> Result<(), ProfileImportError> {
    let source = entry.source_root.join(&entry.source_relative);
    if !source
        .try_exists()
        .map_err(|error| io_error(&source, error))?
    {
        return Ok(());
    }
    let canonical_root = canonicalize(&entry.source_root)?;
    let destination = stage.join(&entry.destination_relative);
    let mut ancestors = HashSet::new();
    copy_allowed_node(
        &source,
        &destination,
        &canonical_root,
        &entry.source_relative,
        &mut ancestors,
        report,
    )
}

fn copy_allowed_node(
    source: &Path,
    destination: &Path,
    source_root: &Path,
    relative: &Path,
    ancestors: &mut HashSet<PathBuf>,
    report: &mut ProfileImportReport,
) -> Result<(), ProfileImportError> {
    if is_excluded(relative) {
        return Ok(());
    }
    let link_metadata = fs::symlink_metadata(source).map_err(|error| io_error(source, error))?;
    if is_link_or_reparse_point(&link_metadata) {
        report.skipped_unsafe_links += 1;
        return Ok(());
    }
    let canonical = canonicalize(source)?;
    if !canonical.starts_with(source_root) {
        return Err(ProfileImportError::SourceEscapes {
            root: source_root.to_path_buf(),
            path: canonical,
        });
    }
    let metadata = fs::metadata(source).map_err(|error| io_error(source, error))?;
    if metadata.is_dir() {
        return copy_allowed_dir(
            source,
            destination,
            source_root,
            relative,
            ancestors,
            report,
        );
    }
    if metadata.is_file() {
        copy_missing_file(source, destination, report)?;
    }
    Ok(())
}

fn copy_allowed_dir(
    source: &Path,
    destination: &Path,
    source_root: &Path,
    relative: &Path,
    ancestors: &mut HashSet<PathBuf>,
    report: &mut ProfileImportReport,
) -> Result<(), ProfileImportError> {
    let canonical = canonicalize(source)?;
    if !ancestors.insert(canonical.clone()) {
        return Err(ProfileImportError::UnsafePath(source.to_path_buf()));
    }
    if !destination.exists() {
        create_dir(destination)?;
    } else if !destination.is_dir() {
        report.skipped_existing += 1;
        ancestors.remove(&canonical);
        return Ok(());
    }
    for child in read_dir(source)? {
        let child = child.map_err(|error| io_error(source, error))?;
        let name = child.file_name();
        copy_allowed_node(
            &child.path(),
            &destination.join(&name),
            source_root,
            &relative.join(&name),
            ancestors,
            report,
        )?;
    }
    ancestors.remove(&canonical);
    Ok(())
}

fn copy_missing_file(
    source: &Path,
    destination: &Path,
    report: &mut ProfileImportReport,
) -> Result<(), ProfileImportError> {
    if destination.exists() {
        report.skipped_existing += 1;
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        create_dir(parent)?;
    }
    fs::copy(source, destination).map_err(|error| io_error(destination, error))?;
    report.imported_files += 1;
    Ok(())
}

fn copy_existing_tree(source: &Path, destination: &Path) -> Result<(), ProfileImportError> {
    for child in read_dir(source)? {
        let child = child.map_err(|error| io_error(source, error))?;
        let source_path = child.path();
        let destination_path = destination.join(child.file_name());
        let metadata =
            fs::symlink_metadata(&source_path).map_err(|error| io_error(&source_path, error))?;
        if metadata.file_type().is_symlink() {
            return Err(ProfileImportError::UnsafePath(source_path));
        }
        if metadata.is_dir() {
            create_dir(&destination_path)?;
            copy_existing_tree(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &destination_path)
                .map_err(|error| io_error(&destination_path, error))?;
        }
    }
    Ok(())
}

fn is_excluded(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(name) = component else {
            return true;
        };
        let name = name.to_string_lossy().to_ascii_lowercase();
        matches!(
            name.as_str(),
            "session"
                | "sessions"
                | "conversation"
                | "conversations"
                | "log"
                | "logs"
                | "cache"
                | "caches"
                | "download"
                | "downloads"
                | "tmp"
                | "temp"
                | "runtime"
                | "runtimes"
                | "node_modules"
                | ".git"
                | "lock"
                | "locks"
        ) || name.ends_with(".lock")
    })
}
