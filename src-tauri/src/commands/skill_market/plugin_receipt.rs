use super::plugin_install_context::PreparedPluginInstall;
use super::plugin_storage_io::{is_link, storage_error, write_bytes_atomically};
use super::SkillPluginManifest;
use crate::app_error::AppCommandError;
use crate::commands::acp::MarketSkillMarker;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_RECEIPT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PluginVersionReceipt {
    pub schema_version: u8,
    pub market: MarketSkillMarker,
    pub manifest: SkillPluginManifest,
    pub publisher_id: String,
    pub signature_key_id: String,
    pub account_scope: String,
    pub plugin_dependencies: Vec<i64>,
    pub recorded_at: String,
}

pub(super) struct ReceiptTransaction {
    path: PathBuf,
    previous: Option<Vec<u8>>,
    bytes: Vec<u8>,
    published: bool,
}

impl ReceiptTransaction {
    pub(super) fn stage(
        roots: (&Path, &Path),
        plugin: &PreparedPluginInstall,
        context: (&[i64], &str, &MarketSkillMarker),
    ) -> Result<Self, AppCommandError> {
        let receipt = PluginVersionReceipt {
            schema_version: 1,
            market: context.2.clone(),
            manifest: plugin.plugin.manifest.clone(),
            publisher_id: plugin.publisher_id.clone(),
            signature_key_id: plugin.signature_key_id.clone(),
            account_scope: context.1.into(),
            plugin_dependencies: context.0.to_vec(),
            recorded_at: chrono::Utc::now().to_rfc3339(),
        };
        let value = Self::from_receipt(roots.0, receipt)?;
        if let Some(previous) = &value.previous {
            write_bytes_atomically(&roots.1.join("receipt-backup.json"), previous)?;
        }
        Ok(value)
    }

    fn from_receipt(root: &Path, receipt: PluginVersionReceipt) -> Result<Self, AppCommandError> {
        let directory = root.join("receipts");
        require_directory(&directory)?;
        let path = directory.join(format!("{}.json", receipt.market.installed_version));
        let previous = read(&path)?;
        let bytes = serde_json::to_vec_pretty(&receipt).map_err(storage_error)?;
        if bytes.len() as u64 > MAX_RECEIPT_BYTES {
            return Err(storage_error(
                "Plugin version receipt exceeds its size limit",
            ));
        }
        Ok(Self {
            path,
            previous,
            bytes,
            published: false,
        })
    }

    pub(super) fn publish(&mut self) -> Result<(), AppCommandError> {
        if read(&self.path)? != self.previous {
            return Err(storage_error(
                "Plugin version receipt changed during installation",
            ));
        }
        write_bytes_atomically(&self.path, &self.bytes)?;
        self.published = true;
        Ok(())
    }

    pub(super) fn rollback(&mut self) -> Result<(), AppCommandError> {
        if self.published {
            if read(&self.path)?.as_deref() != Some(self.bytes.as_slice()) {
                return Err(storage_error("Plugin receipt changed during rollback; original receipt retained in transaction"));
            }
            match &self.previous {
                Some(bytes) => write_bytes_atomically(&self.path, bytes)?,
                None => std::fs::remove_file(&self.path).map_err(storage_error)?,
            }
            self.published = false;
        }
        Ok(())
    }
}

pub(super) async fn capture_previous(
    record: &crate::db::service::plugin_installation_service::PluginInstallationRecord,
) -> Result<(), AppCommandError> {
    if record.installation.schema_version != 2 || record.installation.trust_state != "trusted" {
        return Ok(());
    }
    let path = crate::commands::plugin_recovery::verify_installed_package(record).await?;
    let pointer =
        crate::plugin_runtime::recovery::read_pointer(Path::new(&record.installation.install_root))
            .ok_or_else(|| storage_error("旧插件缺少可靠安装快照，请先恢复安装记录"))?;
    let marker = previous_marker(record, &pointer)?;
    let value = &record.installation;
    let manifest = serde_json::from_str(&value.manifest_json).map_err(storage_error)?;
    let root = path
        .parent()
        .ok_or_else(|| storage_error("旧插件安装路径无效"))?;
    let receipt = PluginVersionReceipt {
        schema_version: 1,
        market: marker,
        manifest,
        publisher_id: value.publisher_id.clone(),
        signature_key_id: value.artifact_signature_key_id.clone(),
        account_scope: pointer.account_scope,
        plugin_dependencies: pointer.plugin_dependencies,
        recorded_at: chrono::Utc::now().to_rfc3339(),
    };
    let mut transaction = ReceiptTransaction::from_receipt(root, receipt)?;
    if transaction.previous.is_none() {
        transaction.publish()?;
    }
    Ok(())
}

fn require_directory(path: &Path) -> Result<(), AppCommandError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => Ok(()),
        Ok(_) => Err(storage_error(
            "Plugin receipt directory must be a real directory",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(path).map_err(storage_error)
        }
        Err(error) => Err(storage_error(error)),
    }
}

pub(super) fn read(path: &Path) -> Result<Option<Vec<u8>>, AppCommandError> {
    use std::io::Read;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(storage_error(error)),
    };
    if !metadata.is_file() || is_link(&metadata) || metadata.len() > MAX_RECEIPT_BYTES {
        return Err(storage_error("Invalid plugin version receipt file"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_RECEIPT_BYTES + 1).read_to_end(&mut bytes))
        .map_err(storage_error)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(storage_error("Plugin receipt changed while reading"));
    }
    Ok(Some(bytes))
}

fn previous_marker(
    record: &crate::db::service::plugin_installation_service::PluginInstallationRecord,
    pointer: &crate::plugin_runtime::recovery::CurrentPointer,
) -> Result<MarketSkillMarker, AppCommandError> {
    let marker = pointer
        .market
        .as_ref()
        .ok_or_else(|| storage_error("旧插件缺少依赖快照，请先恢复安装记录"))?;
    let value = &record.installation;
    if pointer.account_scope.is_empty()
        || pointer.version != value.version
        || pointer.content_sha256 != value.content_sha256
        || pointer.object_sha256 != value.object_sha256
        || marker.schema_version != 3
        || marker.source != "iyw_skill_market"
        || marker.package_type != "plugin"
        || marker.skill_id != value.market_skill_id
        || marker.slug != value.slug
        || marker.installed_version != value.version
        || marker.content_sha256 != value.content_sha256
        || marker.object_sha256 != value.object_sha256
    {
        return Err(storage_error(
            "旧插件安装快照与包身份不符，请先恢复安装记录",
        ));
    }
    Ok(marker.clone())
}
