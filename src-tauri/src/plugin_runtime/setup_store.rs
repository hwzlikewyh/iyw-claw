use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

use super::registry::PluginDescriptor;
use crate::app_error::AppCommandError;
use crate::commands::skill_market::plugin_setup::{self, Preparation};

const MAX_RECORD_BYTES: u64 = 4096;
const MAX_SECRET_BYTES: usize = 256 * 1024;
#[path = "setup_store_cleanup.rs"]
mod cleanup;
pub(crate) use cleanup::{cleanup_records, clear_record, OwnedRecord};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigRecord {
    revision: String,
    secret_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner: Option<ConfigOwner>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigOwner {
    scope: String,
    plugin_id: String,
}

pub(crate) fn contract(
    plugin: &PluginDescriptor,
    connector: &str,
) -> Result<Preparation, AppCommandError> {
    let component = plugin
        .manifest
        .components
        .iter()
        .find(|component| component.kind == "connector" && component.key == connector)
        .ok_or_else(|| invalid("插件连接器不存在"))?;
    match component
        .config
        .as_ref()
        .and_then(|config| config.get("preparation"))
    {
        Some(raw) => plugin_setup::parse(raw),
        None => Ok(Preparation::default()),
    }
}

fn path(plugin: &PluginDescriptor, context: (&str, &str)) -> Result<PathBuf, AppCommandError> {
    let paths = crate::acp::agent_storage::AgentStoragePaths::active()
        .ok_or_else(|| invalid("插件存储不可用"))?;
    let scope = account_scope(plugin)?;
    let key = record_key(&scope, plugin.market_skill_id, context);
    Ok(paths
        .config_dir()
        .join("plugin-preparation")
        .join(format!("{key}.json")))
}

fn account_scope(plugin: &PluginDescriptor) -> Result<String, AppCommandError> {
    super::recovery::read_pointer(std::path::Path::new(&plugin.install_root))
        .map(|pointer| pointer.account_scope)
        .filter(|scope| !scope.is_empty())
        .ok_or_else(|| invalid("插件安装账号范围缺失，请先恢复安装记录"))
}

fn record_key(scope: &str, id: i64, context: (&str, &str)) -> String {
    let identity = serde_json::json!([scope, id.to_string(), context.0, context.1]).to_string();
    format!("{:x}", Sha256::digest(identity.as_bytes()))
}

fn read_record(
    plugin: &PluginDescriptor,
    context: (&str, &str),
) -> Result<Option<ConfigRecord>, AppCommandError> {
    let path = path(plugin, context)?;
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid("插件配置记录无法读取")),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_RECORD_BYTES
    {
        return Err(invalid("插件配置记录无效"));
    }
    let record: ConfigRecord =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| invalid("插件配置记录无法读取"))?)
            .map_err(|_| invalid("插件配置记录损坏，请重新配置"))?;
    if uuid::Uuid::parse_str(&record.revision).is_err()
        || !record.secret_ref.starts_with("plugin-setup:")
        || uuid::Uuid::parse_str(&record.secret_ref[13..]).is_err()
    {
        return Err(invalid("插件配置引用无效"));
    }
    if let Some(owner) = &record.owner {
        if owner.plugin_id != plugin.market_skill_id.to_string()
            || owner.scope != account_scope(plugin)?
        {
            return Err(invalid("插件参数归属与当前安装范围不符"));
        }
    }
    Ok(Some(record))
}

pub(crate) fn revision(plugin: &PluginDescriptor, context: (&str, &str)) -> String {
    match read_record(plugin, context) {
        Ok(Some(record)) => record.revision,
        Ok(None) => String::new(),
        Err(_) => "invalid-configuration".into(),
    }
}

pub(crate) fn stored_values(
    plugin: &PluginDescriptor,
    context: (&str, &str),
) -> Result<BTreeMap<String, String>, AppCommandError> {
    let Some(record) = read_record(plugin, context)? else {
        return Ok(BTreeMap::new());
    };
    let raw = crate::keyring_store::get_connector_secret(&record.secret_ref)
        .ok_or_else(|| invalid("插件安全存储中的参数缺失，请重新配置"))?;
    if raw.len() > MAX_SECRET_BYTES {
        return Err(invalid("插件参数安全记录超过大小限制"));
    }
    serde_json::from_str(&raw).map_err(|_| invalid("插件参数安全记录无效"))
}

pub(crate) fn environment(
    plugin: &PluginDescriptor,
    context: (&str, &str),
) -> Result<BTreeMap<String, String>, AppCommandError> {
    let preparation = contract(plugin, context.1)?;
    if preparation.parameters.is_empty() {
        return Ok(BTreeMap::new());
    }
    plugin_setup::values(&preparation, &stored_values(plugin, context)?)
}

pub(crate) fn save(
    plugin: &PluginDescriptor,
    context: (&str, &str),
    input: (&BTreeMap<String, String>, &[String]),
) -> Result<(), AppCommandError> {
    let preparation = contract(plugin, context.1)?;
    let mut values = stored_values(plugin, context).unwrap_or_default();
    let fields = preparation
        .parameters
        .iter()
        .map(|field| (field.key.as_str(), field))
        .collect::<BTreeMap<_, _>>();
    if input.0.keys().any(|key| !fields.contains_key(key.as_str()))
        || input.1.iter().any(|key| !fields.contains_key(key.as_str()))
    {
        return Err(invalid("配置包含未声明的插件参数"));
    }
    for key in input.1 {
        values.remove(key);
    }
    for (key, value) in input.0 {
        values.insert(key.clone(), value.clone());
    }
    let resolved = plugin_setup::values(&preparation, &values)?;
    let previous = read_record(plugin, context).ok().flatten();
    let record = ConfigRecord {
        revision: uuid::Uuid::new_v4().to_string(),
        secret_ref: format!("plugin-setup:{}", uuid::Uuid::new_v4()),
        owner: Some(ConfigOwner {
            scope: account_scope(plugin)?,
            plugin_id: plugin.market_skill_id.to_string(),
        }),
    };
    let raw = serde_json::to_string(&resolved).map_err(|_| invalid("插件参数无法保存"))?;
    crate::keyring_store::set_connector_secret(&record.secret_ref, &raw)
        .map_err(|_| invalid("插件参数无法写入安全存储"))?;
    if let Err(error) = write_record(plugin, context, &record) {
        let _ = crate::keyring_store::delete_connector_secret(&record.secret_ref);
        return Err(error);
    }
    if let Some(previous) = previous {
        if crate::keyring_store::delete_connector_secret(&previous.secret_ref).is_err() {
            tracing::warn!(plugin=%plugin.slug,"[plugin-setup] 旧的未引用参数安全记录清理失败");
        }
    }
    Ok(())
}

fn write_record(
    plugin: &PluginDescriptor,
    context: (&str, &str),
    record: &ConfigRecord,
) -> Result<(), AppCommandError> {
    let path = path(plugin, context)?;
    let parent = path.parent().ok_or_else(|| invalid("插件配置路径无效"))?;
    std::fs::create_dir_all(parent).map_err(|_| invalid("插件配置目录无法创建"))?;
    if parent
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err(invalid("插件配置目录不能是符号链接"));
    }
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| invalid("插件配置临时文件无法创建"))?;
    serde_json::to_writer(temporary.as_file_mut(), record)
        .map_err(|_| invalid("插件配置无法编码"))?;
    temporary
        .flush()
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| invalid("插件配置无法写入"))?;
    temporary
        .persist(path)
        .map_err(|_| invalid("插件配置无法提交，原配置保留"))?;
    Ok(())
}

fn invalid(message: &str) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}
