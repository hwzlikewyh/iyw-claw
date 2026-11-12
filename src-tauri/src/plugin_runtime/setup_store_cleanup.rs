use super::*;
use crate::db::service::plugin_installation_service::PluginInstallationRecord;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

const MAX_CONFIG_RECORDS: usize = 4096;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OwnedRecord {
    pub name: String,
    pub secret_ref: String,
    pub revision: String,
}

pub(crate) fn cleanup_records(
    record: &PluginInstallationRecord,
    scope: &str,
) -> Result<(Vec<OwnedRecord>, bool), AppCommandError> {
    let root = config_root()?;
    let entries = match std::fs::read_dir(&root) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), false))
        }
        Err(_) => return Err(invalid("插件参数目录无法检查")),
    };
    let keys = legacy_keys(record, scope);
    let mut owned = Vec::new();
    let mut legacy = false;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_CONFIG_RECORDS {
            return Err(invalid("插件参数记录过多，无法完整检查"));
        }
        let entry = entry.map_err(|_| invalid("插件参数记录无法检查"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !valid_name(&name) {
            continue;
        }
        let Some(value) = read(&entry.path())? else {
            continue;
        };
        let matches = value.owner.as_ref().is_some_and(|owner| {
            owner.scope == scope
                && owner.plugin_id == record.installation.market_skill_id.to_string()
        }) || value.owner.is_none() && keys.contains(&name);
        legacy |= value.owner.is_none() && !keys.contains(&name);
        if matches {
            owned.push(OwnedRecord {
                name,
                secret_ref: value.secret_ref,
                revision: value.revision,
            });
        }
    }
    owned.sort_by(|left, right| left.name.cmp(&right.name));
    Ok((owned, legacy))
}

pub(crate) fn clear_record(record: &OwnedRecord) -> Result<(), AppCommandError> {
    if !valid_name(&record.name) || !valid_secret(&record.secret_ref) {
        return Err(invalid("插件清理记录无效"));
    }
    let path = config_root()?.join(&record.name);
    if let Some(current) = read(&path)? {
        if current.secret_ref == record.secret_ref {
            if current.revision != record.revision {
                return Err(invalid("插件参数已变化，旧清理记录未应用"));
            }
            std::fs::remove_file(&path).map_err(|_| invalid("插件参数引用清理失败"))?;
        }
    }
    crate::keyring_store::delete_connector_secret(&record.secret_ref)
        .map_err(|_| invalid("插件安全参数清理失败，可继续清理"))
}

fn legacy_keys(record: &PluginInstallationRecord, scope: &str) -> BTreeSet<String> {
    let workspaces = std::iter::once("")
        .chain(
            record
                .activations
                .iter()
                .map(|value| value.workspace_key.as_str()),
        )
        .chain(
            record
                .permission_grants
                .iter()
                .map(|value| value.workspace_key.as_str()),
        )
        .collect::<BTreeSet<_>>();
    let connectors = record
        .components
        .iter()
        .filter(|component| component.component_type == "connector")
        .map(|component| component.component_key.as_str());
    let mut keys = BTreeSet::new();
    for connector in connectors {
        for workspace in &workspaces {
            keys.insert(format!(
                "{}.json",
                record_key(
                    scope,
                    record.installation.market_skill_id,
                    (workspace, connector)
                )
            ));
        }
    }
    keys
}

fn config_root() -> Result<PathBuf, AppCommandError> {
    let paths = crate::acp::agent_storage::AgentStoragePaths::active()
        .ok_or_else(|| invalid("插件存储不可用"))?;
    let root = paths.config_dir().join("plugin-preparation");
    if let Ok(metadata) = std::fs::symlink_metadata(&root) {
        if !metadata.is_dir() || is_link(&metadata) {
            return Err(invalid("插件参数目录不是受管实体目录"));
        }
    }
    Ok(root)
}

fn read(path: &Path) -> Result<Option<ConfigRecord>, AppCommandError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid("插件参数记录无法读取")),
    };
    if !metadata.is_file() || is_link(&metadata) || metadata.len() > MAX_RECORD_BYTES {
        return Err(invalid("插件参数记录无效"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| invalid("插件参数记录无法读取"))?;
    let record: ConfigRecord =
        serde_json::from_slice(&bytes).map_err(|_| invalid("插件参数记录损坏"))?;
    if !valid_secret(&record.secret_ref) || uuid::Uuid::parse_str(&record.revision).is_err() {
        return Err(invalid("插件参数引用无效"));
    }
    Ok(Some(record))
}

fn valid_secret(value: &str) -> bool {
    value
        .strip_prefix("plugin-setup:")
        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
}
fn valid_name(value: &str) -> bool {
    value.strip_suffix(".json").is_some_and(|name| {
        name.len() == 64
            && name
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE: u32 = 0x0000_0400;
        if metadata.file_attributes() & REPARSE != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}
