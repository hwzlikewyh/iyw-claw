use super::{
    local,
    types::{ListInput, VersionEntry, VersionList},
};
use crate::app_error::AppCommandError;
use sea_orm::DatabaseConnection;

const PAGE_SIZE: usize = 20;
const MAX_VERSIONS: usize = 4096;

pub async fn list(
    conn: &DatabaseConnection,
    input: ListInput,
) -> Result<VersionList, AppCommandError> {
    if input.page == 0 {
        return Err(super::invalid("本地版本页码必须从1开始"));
    }
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = crate::plugin_runtime::registry::lock_mutation().await;
    let record = super::current(conn, &input.market_skill_id).await?;
    tokio::task::spawn_blocking(move || {
        read(
            &record.installation.slug,
            &record.installation.version,
            input.page,
        )
    })
    .await
    .map_err(|_| super::invalid("本地版本列表读取未完成"))?
}

fn read(slug: &str, current: &str, page: u32) -> Result<VersionList, AppCommandError> {
    let mut versions = read_versions(slug)?;
    let active = semver::Version::parse(current).map_err(|_| super::invalid("当前安装版本无效"))?;
    if !versions.contains(&active) {
        versions.push(active);
    }
    versions.sort_by(|a, b| b.cmp(a));
    versions.dedup();
    let start = ((page - 1) as usize)
        .checked_mul(PAGE_SIZE)
        .ok_or_else(|| super::invalid("本地版本页码无效"))?;
    let next_page =
        (versions.len() > start.saturating_add(PAGE_SIZE)).then_some(page.saturating_add(1));
    let versions = versions
        .into_iter()
        .skip(start)
        .take(PAGE_SIZE)
        .map(|version| entry(slug, current, version.to_string()))
        .collect();
    Ok(VersionList {
        slug: slug.into(),
        current_version: current.into(),
        versions,
        next_page,
    })
}

fn entry(slug: &str, current: &str, version: String) -> VersionEntry {
    match local::receipt(slug, &version) {
        Ok(receipt) => VersionEntry {
            current: version == current,
            version,
            recorded_at: Some(receipt.recorded_at),
            available: true,
            issue: None,
        },
        Err(error) => VersionEntry {
            current: version == current,
            version,
            recorded_at: None,
            available: false,
            issue: Some(error.message),
        },
    }
}

fn read_versions(slug: &str) -> Result<Vec<semver::Version>, AppCommandError> {
    let directory = local::root(slug)?.join("receipts");
    let mut versions = Vec::new();
    match std::fs::read_dir(directory) {
        Ok(entries) => {
            for (index, entry) in entries.enumerate() {
                let entry = entry.map_err(|_| super::invalid("本地版本目录读取失败"))?;
                if index >= MAX_VERSIONS {
                    return Err(super::invalid("本地版本过多，请通过安装目录管理后重试"));
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if let Some(version) = name
                    .strip_suffix(".json")
                    .and_then(|value| semver::Version::parse(value).ok())
                {
                    versions.push(version);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(_) => return Err(super::invalid("本地版本目录读取失败")),
    }
    Ok(versions)
}
