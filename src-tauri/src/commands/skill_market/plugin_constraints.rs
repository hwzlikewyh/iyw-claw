use crate::app_error::AppCommandError;
use crate::commands::acp::MarketSkillMarker;
use crate::db::service::plugin_installation_service;
use sea_orm::DatabaseConnection;
use std::collections::BTreeMap;

pub(super) type PlannedVersions = BTreeMap<i64, (String, String)>;

pub(super) async fn report(
    conn: &DatabaseConnection,
    planned: &PlannedVersions,
) -> Result<ConstraintReport, AppCommandError> {
    let records = plugin_installation_service::list_records(conn)
        .await
        .map_err(AppCommandError::db)?;
    let changed = records.iter().any(|record| {
        planned
            .get(&record.installation.market_skill_id)
            .is_some_and(|(_, version)| *version != record.installation.version)
    });
    let mut issues = Vec::new();
    let mut related = BTreeMap::new();
    for record in &records {
        if planned.contains_key(&record.installation.market_skill_id) {
            continue;
        }
        match super::plugin_history::local::pointer(record).and_then(|pointer| {
            pointer
                .and_then(|value| value.market)
                .ok_or_else(|| AppCommandError::configuration_invalid("安装依赖快照缺失"))
        }) {
            Ok(marker) => check(&marker, planned, (&mut issues, &mut related)),
            Err(_) if changed => {
                issues.push(format!(
                    "现有插件 {} 缺少可靠依赖快照，请先恢复或修复安装",
                    record.installation.slug
                ));
                related.insert(
                    record.installation.market_skill_id.to_string(),
                    record.installation.slug.clone(),
                );
            }
            Err(_) => (),
        }
    }
    check_shared(planned, (&mut issues, &mut related))?;
    issues.sort();
    issues.dedup();
    Ok(ConstraintReport { issues, related })
}

const MAX_SHARED_ENTRIES: usize = 4096;

pub(super) async fn removal_issues(
    conn: &DatabaseConnection,
    id: i64,
) -> Result<Vec<String>, AppCommandError> {
    let records = plugin_installation_service::list_records(conn)
        .await
        .map_err(AppCommandError::db)?;
    let mut issues = Vec::new();
    for record in records {
        let marker = super::plugin_history::local::pointer(&record)
            .ok()
            .flatten()
            .and_then(|pointer| pointer.market);
        let Some(marker) = marker.filter(|marker| {
            marker.skill_id == record.installation.market_skill_id
                && marker.slug == record.installation.slug
                && marker.source == "iyw_skill_market"
        }) else {
            continue;
        };
        if marker.skill_id == id {
            if !marker.target_references.is_empty() && !marker.target_references.contains_key(&id) {
                issues.push("此插件作为其它能力的依赖安装，请先从对应能力入口解除依赖".into());
            }
        } else if marker
            .dependencies
            .iter()
            .any(|dependency| dependency.skill_id == id)
        {
            issues.push(format!(
                "插件 {} 仍依赖此安装，请先处理该插件后再移除",
                marker.slug
            ));
        }
    }
    removal_shared(id, &mut issues)?;
    issues.sort();
    issues.dedup();
    Ok(issues)
}

pub(super) struct ConstraintReport {
    pub issues: Vec<String>,
    pub related: BTreeMap<String, String>,
}

pub(super) async fn issues(
    conn: &DatabaseConnection,
    planned: &PlannedVersions,
) -> Result<Vec<String>, AppCommandError> {
    Ok(report(conn, planned).await?.issues)
}

fn check(
    marker: &MarketSkillMarker,
    planned: &PlannedVersions,
    output: (&mut Vec<String>, &mut BTreeMap<String, String>),
) {
    let (issues, related) = output;
    for dependency in &marker.dependencies {
        if planned
            .get(&dependency.skill_id)
            .is_some_and(|(slug, version)| {
                *slug != dependency.slug || *version != dependency.version
            })
        {
            related.insert(marker.skill_id.to_string(), marker.slug.clone());
            issues.push(format!(
                "现有能力 {} 仍需要 {}@{}，请先处理该依赖关系",
                marker.slug, dependency.slug, dependency.version
            ));
        }
    }
}

fn check_shared(
    planned: &PlannedVersions,
    output: (&mut Vec<String>, &mut BTreeMap<String, String>),
) -> Result<(), AppCommandError> {
    let (issues, related) = output;
    let root = crate::commands::acp::shared_skills_dir();
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => Some(entries),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(AppCommandError::io_error("技能依赖目录无法读取")),
    };
    if let Some(entries) = entries {
        for (index, entry) in entries.enumerate() {
            if index >= MAX_SHARED_ENTRIES {
                return Err(AppCommandError::configuration_invalid(
                    "本地技能过多，无法完整检查版本影响",
                ));
            }
            let path = entry
                .map_err(|_| AppCommandError::io_error("技能依赖目录读取失败"))?
                .path();
            if let Some(marker) = super::plugin_history::local::read_marker(&path)? {
                if !planned.contains_key(&marker.skill_id) {
                    check(&marker, planned, (issues, related));
                }
            }
        }
    }
    Ok(())
}

fn removal_shared(id: i64, issues: &mut Vec<String>) -> Result<(), AppCommandError> {
    let root = crate::commands::acp::shared_skills_dir();
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => Some(entries),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Err(AppCommandError::io_error(
                "无法检查技能依赖引用，移除操作已取消",
            ))
        }
    };
    if let Some(entries) = entries {
        for (index, entry) in entries.enumerate() {
            if index >= MAX_SHARED_ENTRIES {
                return Err(AppCommandError::configuration_invalid(
                    "本地技能过多，无法完整检查移除影响",
                ));
            }
            let path = entry
                .map_err(|_| AppCommandError::io_error("技能引用读取失败"))?
                .path();
            if let Some(marker) = super::plugin_history::local::read_marker(&path)? {
                if marker.skill_id != id
                    && marker
                        .dependencies
                        .iter()
                        .any(|dependency| dependency.skill_id == id)
                {
                    issues.push(format!(
                        "能力 {} 仍依赖此安装，请先处理该依赖后再移除",
                        marker.slug
                    ));
                }
            }
        }
    }
    Ok(())
}
