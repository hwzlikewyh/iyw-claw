use crate::app_error::AppCommandError;
use crate::plugin_runtime::setup_store::{self, OwnedRecord};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Plan {
    pub scope: String,
    pub slug: String,
    pub id: String,
    pub records: Vec<OwnedRecord>,
    pub available: bool,
    pub legacy_retained: bool,
    #[serde(default)]
    pub storage_root: String,
    #[serde(default)]
    pub package_trash: Option<String>,
    #[serde(default)]
    pub previous_trash: Vec<String>,
}

pub(super) async fn plan(
    conn: &sea_orm::DatabaseConnection,
    prepared: &super::types::Prepared,
) -> Result<Plan, AppCommandError> {
    let root = &prepared.root;
    let available = super::super::plugin_history::local::pointer(root)
        .ok()
        .flatten()
        .is_some_and(|pointer| pointer.account_scope == prepared.scope);
    let mut plan = Plan {
        scope: prepared.scope.clone(),
        slug: root.installation.slug.clone(),
        id: root.installation.market_skill_id.to_string(),
        available,
        storage_root: super::super::plugin_storage_io::active_paths()?
            .root()
            .to_string_lossy()
            .into_owned(),
        ..Default::default()
    };
    if available {
        let record = root.clone();
        let scope = prepared.scope.clone();
        match tokio::task::spawn_blocking(move || setup_store::cleanup_records(&record, &scope))
            .await
        {
            Ok(Ok((records, legacy))) => {
                plan.records = records;
                plan.legacy_retained = legacy;
            }
            _ => {
                plan.available = false;
                plan.legacy_retained = true;
            }
        }
    }
    merge_previous(conn, &mut plan).await?;
    Ok(plan)
}

pub(super) fn purge(plan: &Plan) -> Result<(), AppCommandError> {
    require_root(plan)?;
    if !plan.available {
        return Err(super::invalid("当前安装账号归属无法确认，已保留私有数据"));
    }
    let path = data_path(plan)?;
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_dir() || super::super::plugin_storage_io::is_link(&metadata) {
                return Err(super::invalid("插件私有数据目录不是受管实体目录"));
            }
            std::fs::remove_dir_all(&path)
                .map_err(|_| super::invalid("插件私有数据清理未完成，可继续清理"))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(_) => return Err(super::invalid("插件私有数据无法读取，清理未完成")),
    }
    for record in &plan.records {
        setup_store::clear_record(record)?;
    }
    Ok(())
}

pub(super) fn package(plan: &Plan) -> Result<(), AppCommandError> {
    require_root(plan)?;
    for name in plan.package_trash.iter().chain(plan.previous_trash.iter()) {
        package_entry(plan, name)?;
    }
    Ok(())
}

fn package_entry(plan: &Plan, name: &str) -> Result<(), AppCommandError> {
    let uuid = name
        .strip_prefix(&format!("{}-", plan.slug))
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .ok_or_else(|| super::invalid("插件待清理目录身份无效"))?;
    let paths = super::super::plugin_storage_io::active_paths()?;
    let root = paths.trash_dir().join("plugins");
    if let Ok(metadata) = std::fs::symlink_metadata(&root) {
        if !metadata.is_dir() || super::super::plugin_storage_io::is_link(&metadata) {
            return Err(super::invalid("插件待清理目录无效"));
        }
    }
    let path = root.join(format!("{}-{uuid}", plan.slug));
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_dir() || super::super::plugin_storage_io::is_link(&metadata) {
                return Err(super::invalid("插件待清理目录不是实体目录"));
            }
            std::fs::remove_dir_all(path)
                .map_err(|_| super::invalid("插件包清理未完成，可继续清理"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(super::invalid("插件待清理目录无法读取")),
    }
}

pub(super) fn require_root(plan: &Plan) -> Result<(), AppCommandError> {
    if super::super::plugin_storage_io::active_paths()?
        .root()
        .to_string_lossy()
        != plan.storage_root
    {
        return Err(super::invalid("能力存储位置已变化，旧清理记录未应用"));
    }
    Ok(())
}

fn data_path(plan: &Plan) -> Result<std::path::PathBuf, AppCommandError> {
    let paths = super::super::plugin_storage_io::active_paths()?;
    super::super::plugin_history::local::root(&plan.slug)?;
    if plan.scope.len() != 64 || !plan.scope.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(super::invalid("账号清理范围无效"));
    }
    let root = paths.root().join("plugin-data");
    let scope = root.join(format!("{:x}", Sha256::digest(plan.scope.as_bytes())));
    for parent in [&root, &scope] {
        if let Ok(metadata) = std::fs::symlink_metadata(parent) {
            if !metadata.is_dir() || super::super::plugin_storage_io::is_link(&metadata) {
                return Err(super::invalid("插件私有数据范围不是受管实体目录"));
            }
        }
    }
    Ok(scope.join(&plan.slug))
}

async fn merge_previous(
    conn: &sea_orm::DatabaseConnection,
    plan: &mut Plan,
) -> Result<(), AppCommandError> {
    if let Some(previous) = super::journal::read(conn, &plan.id, &plan.scope)
        .await?
        .filter(|value| {
            value.phase != "complete"
                && value.plan.slug == plan.slug
                && value.plan.storage_root == plan.storage_root
        })
    {
        plan.previous_trash.extend(previous.plan.package_trash);
        plan.previous_trash.extend(previous.plan.previous_trash);
        plan.previous_trash.sort();
        plan.previous_trash.dedup();
        for record in previous.plan.records {
            if !plan
                .records
                .iter()
                .any(|value| value.secret_ref == record.secret_ref)
            {
                plan.records.push(record);
            }
        }
    }
    Ok(())
}
