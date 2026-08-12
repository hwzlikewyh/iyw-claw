use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::skill_package::ValidatedSkillPackage;
use crate::app_error::AppCommandError;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrentPluginVersion<'a> {
    version: &'a str,
    content_sha256: &'a str,
    object_sha256: &'a str,
}

pub(super) struct PluginStorageTransaction {
    transaction_root: PathBuf,
    payload: PathBuf,
    backup: PathBuf,
    plugin_root: PathBuf,
    version_dir: PathBuf,
    previous_current: Option<Vec<u8>>,
    committed: bool,
}

impl PluginStorageTransaction {
    pub(super) fn stage(
        package: &ValidatedSkillPackage,
        slug: &str,
        version: &str,
    ) -> Result<Self, AppCommandError> {
        let paths = active_paths()?;
        let transaction_root = paths
            .staging_dir()
            .join("plugins")
            .join(uuid::Uuid::new_v4().to_string());
        let payload = transaction_root.join("payload");
        if let Err(error) = write_package(&payload, package) {
            let _ = fs::remove_dir_all(&transaction_root);
            return Err(error);
        }
        let plugin_root = paths.plugins_dir().join(slug);
        let version_dir = plugin_root.join("versions").join(version);
        Ok(Self {
            backup: transaction_root.join("backup"),
            transaction_root,
            payload,
            plugin_root,
            version_dir,
            previous_current: None,
            committed: false,
        })
    }

    pub(super) fn version_dir(&self) -> &Path {
        &self.version_dir
    }

    pub(super) fn commit(
        &mut self,
        version: &str,
        content_sha256: &str,
        object_sha256: &str,
    ) -> Result<(), AppCommandError> {
        let versions = self.plugin_root.join("versions");
        fs::create_dir_all(&versions).map_err(storage_error)?;
        if self.version_dir.exists() {
            fs::rename(&self.version_dir, &self.backup).map_err(storage_error)?;
        }
        if let Err(error) = fs::rename(&self.payload, &self.version_dir) {
            restore_backup(&self.backup, &self.version_dir);
            return Err(storage_error(error));
        }
        self.committed = true;
        let current_path = self.plugin_root.join("current.json");
        self.previous_current = fs::read(&current_path).ok();
        let current = CurrentPluginVersion {
            version,
            content_sha256,
            object_sha256,
        };
        if let Err(error) = write_json_atomically(&current_path, &current) {
            self.rollback();
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn rollback(&mut self) {
        if self.committed {
            let _ = fs::remove_dir_all(&self.version_dir);
            restore_backup(&self.backup, &self.version_dir);
            restore_current(&self.plugin_root, self.previous_current.as_deref());
            self.committed = false;
        }
        let _ = fs::remove_dir_all(&self.transaction_root);
    }

    pub(super) fn finish(mut self) {
        self.committed = false;
        let _ = fs::remove_dir_all(&self.transaction_root);
    }
}

pub(super) struct PluginStorageRemoval {
    plugin_root: PathBuf,
    trash_root: PathBuf,
    moved: bool,
}

impl PluginStorageRemoval {
    pub(super) fn stage(slug: &str) -> Result<Self, AppCommandError> {
        let paths = active_paths()?;
        let plugin_root = paths.plugins_dir().join(slug);
        let trash_root = paths
            .trash_dir()
            .join("plugins")
            .join(format!("{slug}-{}", uuid::Uuid::new_v4()));
        if plugin_root.exists() {
            if let Some(parent) = trash_root.parent() {
                fs::create_dir_all(parent).map_err(storage_error)?;
            }
            fs::rename(&plugin_root, &trash_root).map_err(storage_error)?;
        }
        Ok(Self {
            plugin_root,
            moved: trash_root.exists(),
            trash_root,
        })
    }

    pub(super) fn rollback(&mut self) {
        if self.moved && !self.plugin_root.exists() {
            let _ = fs::rename(&self.trash_root, &self.plugin_root);
            self.moved = false;
        }
    }

    pub(super) fn finish(mut self) {
        if self.moved {
            let _ = fs::remove_dir_all(&self.trash_root);
            self.moved = false;
        }
    }
}

fn active_paths() -> Result<AgentStoragePaths, AppCommandError> {
    AgentStoragePaths::active().ok_or_else(|| {
        AppCommandError::agent_storage_not_initialized("Agent storage is not initialized")
    })
}

fn write_package(root: &Path, package: &ValidatedSkillPackage) -> Result<(), AppCommandError> {
    for file in &package.files {
        let target = root.join(&file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(storage_error)?;
        }
        fs::write(target, &file.bytes).map_err(storage_error)?;
    }
    Ok(())
}

fn write_json_atomically(target: &Path, value: &impl Serialize) -> Result<(), AppCommandError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        AppCommandError::configuration_invalid("Failed to serialize plugin activation pointer")
            .with_detail(error.to_string())
    })?;
    let temporary = target.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    fs::write(&temporary, bytes).map_err(storage_error)?;
    if target.exists() {
        fs::remove_file(target).map_err(storage_error)?;
    }
    if let Err(error) = fs::rename(&temporary, target) {
        let _ = fs::remove_file(temporary);
        return Err(storage_error(error));
    }
    Ok(())
}

fn restore_backup(backup: &Path, target: &Path) {
    if backup.exists() && !target.exists() {
        let _ = fs::rename(backup, target);
    }
}

fn restore_current(plugin_root: &Path, previous: Option<&[u8]>) {
    let current = plugin_root.join("current.json");
    match previous {
        Some(bytes) => {
            let _ = fs::write(current, bytes);
        }
        None => {
            let _ = fs::remove_file(current);
        }
    }
}

fn storage_error(error: impl ToString) -> AppCommandError {
    AppCommandError::io_error("Failed to update managed plugin storage")
        .with_detail(error.to_string())
}
