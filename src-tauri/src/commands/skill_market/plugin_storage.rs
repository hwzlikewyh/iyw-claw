use super::plugin_storage_io::{
    active_paths, restore_backup, restore_current, storage_error, write_json_atomically,
    write_package,
};
pub(super) use super::plugin_storage_removal::PluginStorageRemoval;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::plugin_install_context::PreparedPluginInstall;
use super::plugin_receipt::ReceiptTransaction;
use crate::acp::skill_package::ValidatedSkillPackage;
use crate::app_error::AppCommandError;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrentPluginVersion<'a> {
    version: &'a str,
    content_sha256: &'a str,
    object_sha256: &'a str,
    market: &'a crate::commands::acp::MarketSkillMarker,
    plugin_dependencies: &'a [i64],
    account_scope: &'a str,
}

pub(super) struct PluginStorageTransaction {
    transaction_root: PathBuf,
    payload: PathBuf,
    backup: PathBuf,
    plugin_root: PathBuf,
    version_dir: PathBuf,
    previous_current: Option<Vec<u8>>,
    committed: bool,
    pointer_pending: bool,
    resolved: bool,
    receipt: Option<ReceiptTransaction>,
}

impl PluginStorageTransaction {
    pub(super) fn stage(
        package: &ValidatedSkillPackage,
        slug: &str,
        version: &str,
    ) -> Result<Self, AppCommandError> {
        let paths = active_paths()?;
        let plugin_root = paths.plugins_dir().join(slug);
        super::plugin_storage_io::require_plugin_path(&plugin_root)?;
        if semver::Version::parse(version).is_err() {
            return Err(storage_error("Invalid plugin version directory"));
        }
        let transaction_root = paths
            .staging_dir()
            .join("plugins")
            .join(uuid::Uuid::new_v4().to_string());
        let payload = transaction_root.join("payload");
        if let Err(error) = write_package(&payload, package) {
            let _ = fs::remove_dir_all(&transaction_root);
            return Err(error);
        }
        let version_dir = plugin_root.join("versions").join(version);
        Ok(Self {
            backup: transaction_root.join("backup"),
            transaction_root,
            payload,
            plugin_root,
            version_dir,
            previous_current: None,
            committed: false,
            pointer_pending: false,
            resolved: false,
            receipt: None,
        })
    }

    pub(super) fn version_dir(&self) -> &Path {
        &self.version_dir
    }

    pub(super) fn previous_scope_matches(&self, scope: &str) -> bool {
        self.previous_current
            .as_deref()
            .and_then(|bytes| {
                serde_json::from_slice::<crate::plugin_runtime::recovery::CurrentPointer>(bytes)
                    .ok()
            })
            .is_some_and(|pointer| !scope.is_empty() && pointer.account_scope == scope)
    }

    pub(super) fn commit(
        &mut self,
        plugin: &PreparedPluginInstall,
        context: (&[i64], &str),
    ) -> Result<(), AppCommandError> {
        super::plugin_storage_io::require_plugin_path(&self.plugin_root)?;
        let versions = self.plugin_root.join("versions");
        fs::create_dir_all(&versions).map_err(storage_error)?;
        let current_path = self.plugin_root.join("current.json");
        self.previous_current = super::plugin_storage_io::read_pointer_bytes(&current_path)?;
        let marker = self.merged_marker(&plugin.marker);
        self.receipt = Some(ReceiptTransaction::stage(
            (&self.plugin_root, &self.transaction_root),
            plugin,
            (context.0, context.1, &marker),
        )?);
        if self.version_dir.exists() {
            fs::rename(&self.version_dir, &self.backup).map_err(storage_error)?;
        }
        if let Err(error) = fs::rename(&self.payload, &self.version_dir) {
            restore_backup(&self.backup, &self.version_dir).map_err(|rollback| {
                storage_error(format!("{error}; rollback incomplete: {rollback}"))
            })?;
            return Err(storage_error(error));
        }
        self.committed = true;
        self.pointer_pending = true;
        let current = CurrentPluginVersion {
            version: &marker.installed_version,
            content_sha256: &marker.content_sha256,
            object_sha256: &marker.object_sha256,
            market: &marker,
            plugin_dependencies: context.0,
            account_scope: context.1,
        };
        if let Err(error) = write_json_atomically(&current_path, &current) {
            self.rollback().map_err(|rollback| {
                storage_error(format!("{error}; rollback incomplete: {rollback}"))
            })?;
            return Err(error);
        }
        Ok(())
    }

    fn merged_marker(
        &self,
        marker: &crate::commands::acp::MarketSkillMarker,
    ) -> crate::commands::acp::MarketSkillMarker {
        let mut marker = marker.clone();
        if let Some(previous) = self
            .previous_current
            .as_deref()
            .and_then(|bytes| {
                serde_json::from_slice::<crate::plugin_runtime::recovery::CurrentPointer>(bytes)
                    .ok()
            })
            .and_then(|pointer| pointer.market)
            .filter(|previous| {
                previous.skill_id == marker.skill_id
                    && previous.source == "iyw_skill_market"
                    && previous.slug == marker.slug
            })
        {
            let mut references = previous.target_references;
            references.extend(marker.target_references);
            marker.target_references = references;
            marker.agent_types = Some(
                marker
                    .target_references
                    .values()
                    .flatten()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            );
        }
        marker
    }

    pub(super) fn rollback(&mut self) -> Result<(), AppCommandError> {
        super::plugin_storage_io::require_plugin_path(&self.plugin_root)?;
        if let Some(receipt) = &mut self.receipt {
            receipt.rollback()?;
        }
        if self.committed {
            if self.version_dir.exists() {
                fs::remove_dir_all(&self.version_dir).map_err(storage_error)?;
            }
            restore_backup(&self.backup, &self.version_dir)?;
            self.committed = false;
        } else {
            restore_backup(&self.backup, &self.version_dir)?;
        }
        if self.pointer_pending {
            restore_current(&self.plugin_root, self.previous_current.as_deref())?;
            self.pointer_pending = false;
        }
        if self.transaction_root.exists() {
            fs::remove_dir_all(&self.transaction_root).map_err(storage_error)?;
        }
        self.resolved = true;
        Ok(())
    }

    pub(super) fn publish_receipt(&mut self) -> Result<(), AppCommandError> {
        self.receipt
            .as_mut()
            .ok_or_else(|| storage_error("Plugin receipt was not staged"))?
            .publish()
    }

    pub(super) fn finish(mut self) {
        self.resolved = true;
        self.committed = false;
        if let Err(error) = fs::remove_dir_all(&self.transaction_root) {
            tracing::warn!(error=%error,path=%self.transaction_root.display(),"[plugin-install] 已提交的安装临时目录未清理");
        }
    }
}

impl Drop for PluginStorageTransaction {
    fn drop(&mut self) {
        if !self.resolved {
            if let Err(error) = self.rollback() {
                tracing::error!(path=%self.transaction_root.display(),error=%error,detail=?error.detail,"[plugin-install] 中断事务回退失败，保留备份");
            }
        }
    }
}
