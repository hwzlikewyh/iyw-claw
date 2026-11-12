use super::super::{
    plugin_receipt::{self, PluginVersionReceipt},
    plugin_storage_io::{active_paths, read_pointer_bytes, require_plugin_path},
};
use crate::app_error::AppCommandError;
use crate::db::service::plugin_installation_service::PluginInstallationRecord;
use crate::plugin_runtime::recovery::CurrentPointer;
use std::path::{Path, PathBuf};

pub(crate) fn root(slug: &str) -> Result<PathBuf, AppCommandError> {
    if crate::commands::acp::validate_skill_id(slug).map_err(|_| super::invalid("插件名称无效"))?
        != slug
    {
        return Err(super::invalid("插件名称无效"));
    }
    let root = active_paths()?.plugins_dir().join(slug);
    require_plugin_path(&root)?;
    Ok(root)
}

pub(super) fn receipt(slug: &str, version: &str) -> Result<PluginVersionReceipt, AppCommandError> {
    if semver::Version::parse(version)
        .map_err(|_| super::invalid("插件版本无效"))?
        .to_string()
        != version
    {
        return Err(super::invalid("插件版本无效"));
    }
    let bytes =
        plugin_receipt::read(&root(slug)?.join("receipts").join(format!("{version}.json")))?
            .ok_or_else(|| super::invalid("此本地版本缺少已提交收据，请使用原安装入口"))?;
    let value: PluginVersionReceipt =
        serde_json::from_slice(&bytes).map_err(|_| super::invalid("本地版本收据损坏"))?;
    if value.schema_version != 1
        || value.market.source != "iyw_skill_market"
        || value.market.schema_version != 3
        || value.market.package_type != "plugin"
        || value.market.slug != slug
        || value.market.installed_version != version
        || value.manifest.name != slug
        || value.manifest.version != version
        || value.account_scope.is_empty()
    {
        return Err(super::invalid("本地版本收据与包身份不符"));
    }
    Ok(value)
}

pub(super) fn current_receipt(
    record: &PluginInstallationRecord,
) -> Result<PluginVersionReceipt, AppCommandError> {
    let value = &record.installation;
    let pointer =
        pointer(record)?.ok_or_else(|| super::invalid("依赖插件缺少安装快照，请先恢复安装记录"))?;
    let market = pointer
        .market
        .ok_or_else(|| super::invalid("依赖插件缺少依赖快照"))?;
    if market.schema_version != 3
        || market.source != "iyw_skill_market"
        || market.package_type != "plugin"
        || market.slug != value.slug
        || pointer.version != value.version
        || pointer.content_sha256 != value.content_sha256
        || pointer.object_sha256 != value.object_sha256
        || market.skill_id != value.market_skill_id
        || market.installed_version != value.version
        || pointer.account_scope.is_empty()
        || market.content_sha256 != value.content_sha256
        || market.object_sha256 != value.object_sha256
    {
        return Err(super::invalid("依赖插件安装快照与记录不符"));
    }
    Ok(PluginVersionReceipt {
        schema_version: 1,
        market,
        manifest: serde_json::from_str(&value.manifest_json)
            .map_err(|_| super::invalid("插件清单记录损坏"))?,
        publisher_id: value.publisher_id.clone(),
        signature_key_id: value.artifact_signature_key_id.clone(),
        account_scope: pointer.account_scope,
        plugin_dependencies: pointer.plugin_dependencies,
        recorded_at: value.updated_at.to_rfc3339(),
    })
}

pub(super) fn package(
    receipt: &PluginVersionReceipt,
) -> Result<crate::acp::skill_package::ValidatedSkillPackage, AppCommandError> {
    let path = root(&receipt.market.slug)?
        .join("versions")
        .join(&receipt.market.installed_version);
    crate::acp::skill_package::validate_plugin_directory(&path, &receipt.market.content_sha256)
}

pub(crate) fn pointer(
    record: &PluginInstallationRecord,
) -> Result<Option<CurrentPointer>, AppCommandError> {
    read_pointer_bytes(&root(&record.installation.slug)?.join("current.json"))?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|_| super::invalid("当前插件指针损坏，请先恢复安装记录"))
        })
        .transpose()
}

pub(super) fn candidate(
    current: &PluginInstallationRecord,
    receipt: &PluginVersionReceipt,
) -> Result<PluginInstallationRecord, AppCommandError> {
    let mut record = current.clone();
    record.installation.version = receipt.market.installed_version.clone();
    record.installation.content_sha256 = receipt.market.content_sha256.clone();
    record.installation.object_sha256 = receipt.market.object_sha256.clone();
    record.installation.manifest_json =
        serde_json::to_string(&receipt.manifest).map_err(|_| super::invalid("历史清单无法编码"))?;
    record.installation.install_root = root(&receipt.market.slug)?
        .join("versions")
        .join(&receipt.market.installed_version)
        .to_string_lossy()
        .into();
    Ok(record)
}

pub(crate) fn read_marker(
    path: &Path,
) -> Result<Option<crate::commands::acp::MarketSkillMarker>, AppCommandError> {
    if path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
    {
        return Ok(None);
    }
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(super::invalid("技能目录无法读取")),
    };
    if !metadata.is_dir() || super::super::plugin_storage_io::is_link(&metadata) {
        return Ok(None);
    }
    read_pointer_bytes(&path.join(".iyw-claw-market-skill.json"))?
        .map(|bytes| serde_json::from_slice(&bytes).map_err(|_| super::invalid("技能安装记录损坏")))
        .transpose()
}
