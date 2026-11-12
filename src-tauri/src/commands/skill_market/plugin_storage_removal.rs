use super::plugin_storage_io::{active_paths, storage_error};
use crate::app_error::AppCommandError;
use std::fs;
use std::path::PathBuf;

pub(super) struct PluginStorageRemoval {
    plugin_root: PathBuf,
    trash_root: PathBuf,
    moved: bool,
}

impl PluginStorageRemoval {
    pub(super) fn stage(slug: &str) -> Result<Self, AppCommandError> {
        let mut value = Self::reserve(slug)?;
        value.move_package()?;
        Ok(value)
    }

    pub(super) fn reserve(slug: &str) -> Result<Self, AppCommandError> {
        let paths = active_paths()?;
        let plugin_root = paths.plugins_dir().join(slug);
        super::plugin_storage_io::require_plugin_path(&plugin_root)?;
        let trash_root = paths
            .trash_dir()
            .join("plugins")
            .join(format!("{slug}-{}", uuid::Uuid::new_v4()));
        Ok(Self {
            plugin_root,
            trash_root,
            moved: false,
        })
    }

    pub(super) fn move_package(&mut self) -> Result<(), AppCommandError> {
        super::plugin_storage_io::require_plugin_path(&self.plugin_root)?;
        if self.plugin_root.exists() {
            if let Some(parent) = self.trash_root.parent() {
                fs::create_dir_all(parent).map_err(storage_error)?;
            }
            fs::rename(&self.plugin_root, &self.trash_root).map_err(storage_error)?;
            self.moved = true;
        }
        Ok(())
    }

    pub(super) fn rollback(&mut self) -> Result<(), AppCommandError> {
        super::plugin_storage_io::require_plugin_path(&self.plugin_root)?;
        if self.moved && !self.plugin_root.exists() {
            fs::rename(&self.trash_root, &self.plugin_root).map_err(storage_error)?;
            self.moved = false;
        } else if self.moved {
            return Err(storage_error(
                "Plugin root already exists; removal backup retained",
            ));
        }
        Ok(())
    }

    pub(super) fn cleanup_name(&self) -> Option<String> {
        (self.moved || self.plugin_root.exists())
            .then(|| {
                self.trash_root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .flatten()
    }

    pub(super) fn recover(slug: &str, name: Option<&str>) -> Result<(), AppCommandError> {
        let Some(name) = name else {
            return Ok(());
        };
        let suffix = name
            .strip_prefix(&format!("{slug}-"))
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
            .ok_or_else(|| storage_error("Invalid removal backup identity"))?;
        let mut value = Self::reserve(slug)?;
        value.trash_root = active_paths()?
            .trash_dir()
            .join("plugins")
            .join(format!("{slug}-{suffix}"));
        value.moved = value.trash_root.exists();
        if value.moved {
            let metadata = fs::symlink_metadata(&value.trash_root).map_err(storage_error)?;
            if !metadata.is_dir() || super::plugin_storage_io::is_link(&metadata) {
                return Err(storage_error("Invalid removal backup directory"));
            }
        }
        value.rollback()
    }

    pub(super) fn finish(mut self) {
        if self.moved {
            if let Err(error) = fs::remove_dir_all(&self.trash_root) {
                tracing::warn!(error=%error,path=%self.trash_root.display(),"[plugin-install] 卸载完成，待清理目录保留");
            }
            self.moved = false;
        }
    }
}
