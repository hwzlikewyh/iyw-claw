use std::fs;
use std::path::{Path, PathBuf};

use crate::acp::profile_import::{ProfileImportError, ProfileImportReport};
use crate::acp::profile_import_fs::PreparedProfile;
use crate::acp::profile_import_io::{create_dir, io_error};

struct ActivatedProfile {
    key: &'static str,
    destination: PathBuf,
    backup: Option<PathBuf>,
}

pub(super) fn activate_profiles(
    operation: &Path,
    prepared: Vec<PreparedProfile>,
) -> Result<ProfileImportReport, ProfileImportError> {
    let mut activated = Vec::new();
    let mut report = ProfileImportReport::default();
    for profile in prepared {
        let backup = move_destination_to_backup(operation, &profile)?;
        if let Err(error) = fs::rename(&profile.stage, &profile.destination) {
            restore_current_backup(&profile.destination, backup.as_deref())?;
            rollback_profiles(operation, &activated)?;
            let _ = fs::remove_dir_all(operation);
            return Err(io_error(&profile.destination, error));
        }
        report.imported_files += profile.report.imported_files;
        report.skipped_existing += profile.report.skipped_existing;
        report.skipped_unsafe_links += profile.report.skipped_unsafe_links;
        activated.push(ActivatedProfile {
            key: profile.key,
            destination: profile.destination,
            backup,
        });
    }
    let _ = fs::remove_dir_all(operation);
    Ok(report)
}

fn move_destination_to_backup(
    operation: &Path,
    profile: &PreparedProfile,
) -> Result<Option<PathBuf>, ProfileImportError> {
    if !profile.destination.exists() {
        if let Some(parent) = profile.destination.parent() {
            create_dir(parent)?;
        }
        return Ok(None);
    }
    let backup = operation.join("backups").join(profile.key);
    if let Some(parent) = backup.parent() {
        create_dir(parent)?;
    }
    fs::rename(&profile.destination, &backup)
        .map_err(|error| io_error(&profile.destination, error))?;
    Ok(Some(backup))
}

fn restore_current_backup(
    destination: &Path,
    backup: Option<&Path>,
) -> Result<(), ProfileImportError> {
    if let Some(backup) = backup {
        fs::rename(backup, destination).map_err(|error| {
            ProfileImportError::Activation(format!(
                "restore {} failed: {error}",
                destination.display()
            ))
        })?;
    }
    Ok(())
}

fn rollback_profiles(
    operation: &Path,
    activated: &[ActivatedProfile],
) -> Result<(), ProfileImportError> {
    for profile in activated.iter().rev() {
        let displaced = operation.join("rollback").join(profile.key);
        if let Some(parent) = displaced.parent() {
            create_dir(parent)?;
        }
        fs::rename(&profile.destination, &displaced)
            .map_err(|error| io_error(&profile.destination, error))?;
        restore_current_backup(&profile.destination, profile.backup.as_deref())?;
    }
    Ok(())
}
