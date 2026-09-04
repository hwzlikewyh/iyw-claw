pub(crate) mod client;
mod install;
mod install_plan;
mod plugin_components;
mod plugin_install;
mod plugin_install_context;
mod plugin_install_data;
mod plugin_install_rollback;
mod plugin_install_runtime_state;
mod plugin_manifest;
mod plugin_manifest_v2;
mod plugin_storage;
mod plugin_types;
mod routing_description;
mod types;

pub use install::{install_core, revalidate_artifact_core};

pub const MAX_SKILL_MARKET_REQUEST_BYTES: usize = 36 * 1024 * 1024;

use reqwest::Method;
use sea_orm::DatabaseConnection;
use std::collections::HashSet;

use crate::app_error::AppCommandError;
pub use plugin_types::{
    SkillPluginBinding, SkillPluginComponent, SkillPluginManifest, SkillPluginPermissions,
};
pub(crate) use types::parse_value;
use types::{parse_id, FileNode, FileTree, SkillMarketFile, SkillMarketItem};
pub use types::{
    SkillDependencyInput, SkillMarketAddVersionRequest, SkillMarketCategory, SkillMarketDetail,
    SkillMarketListParams, SkillMarketListResult, SkillMarketMetadataRequest,
    SkillMarketPublishRequest, SkillMarketVersion, SkillPackageType,
};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillAvailability {
    id: String,
    slug: Option<String>,
    status: String,
}

const AVAILABILITY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const AVAILABILITY_BATCH_SIZE: usize = 256;
const AVAILABILITY_RETRY_DELAYS: [std::time::Duration; 3] = [
    std::time::Duration::from_secs(5),
    std::time::Duration::from_secs(30),
    std::time::Duration::from_secs(120),
];

pub fn spawn_background_availability_reconcile(conn: DatabaseConnection) {
    static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if RUNNING
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        )
        .is_err()
    {
        return;
    }
    tokio::spawn(async move {
        let result = reconcile_market_skill_availability_with_retry(&conn).await;
        let deferred = result.as_ref().ok().copied().unwrap_or(false);
        if deferred {
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            if let Err(error) = reconcile_market_skill_availability_with_retry(&conn).await {
                tracing::warn!(
                    error = %error,
                    "[skill-market] deferred availability cleanup retry failed"
                );
            }
        }
        RUNNING.store(false, std::sync::atomic::Ordering::Release);
        if let Err(error) = result {
            tracing::warn!(error = %error, "[skill-market] background availability reconcile skipped");
        }
    });
}

async fn reconcile_market_skill_availability_with_retry(
    conn: &DatabaseConnection,
) -> Result<bool, AppCommandError> {
    let mut last_error = None;
    for attempt in 0..=AVAILABILITY_RETRY_DELAYS.len() {
        match reconcile_market_skill_availability(conn).await {
            Ok(deferred) => return Ok(deferred),
            Err(error) => {
                if !matches!(
                    error.code,
                    crate::app_error::AppErrorCode::NetworkError
                        | crate::app_error::AppErrorCode::DatabaseError
                ) {
                    return Err(error);
                }
                last_error = Some(error);
                if let Some(delay) = AVAILABILITY_RETRY_DELAYS.get(attempt) {
                    tokio::time::sleep(*delay).await;
                }
            }
        }
    }
    Err(last_error.expect("availability retry loop always records an error"))
}

async fn reconcile_market_skill_availability(
    conn: &DatabaseConnection,
) -> Result<bool, AppCommandError> {
    let ids = installed_market_skill_ids(conn).await?;
    if ids.is_empty() {
        return Ok(false);
    }
    let mut deferred = false;
    for chunk in ids.chunks(AVAILABILITY_BATCH_SIZE) {
        deferred |= reconcile_market_skill_availability_batch(conn, chunk).await?;
    }
    Ok(deferred)
}

async fn installed_market_skill_ids(
    conn: &DatabaseConnection,
) -> Result<Vec<i64>, AppCommandError> {
    let mut ids = crate::commands::acp::installed_market_skill_ids();
    let plugin_ids = crate::db::service::plugin_installation_service::list_installations(conn)
        .await
        .map_err(AppCommandError::db)?
        .into_iter()
        .map(|installation| installation.market_skill_id);
    ids.extend(plugin_ids);
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

async fn reconcile_market_skill_availability_batch(
    conn: &DatabaseConnection,
    ids: &[i64],
) -> Result<bool, AppCommandError> {
    let body = serde_json::json!({
        "ids": ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
    });
    let builder = client::request(conn, reqwest::Method::POST, "/skills/availability")
        .await?
        .json(&body)
        .timeout(AVAILABILITY_TIMEOUT);
    let items: Vec<SkillAvailability> = parse_value(client::send(builder).await?, Some("items"))?;
    let mut deferred = false;
    let requested_ids = ids.iter().copied().collect::<HashSet<_>>();
    for item in items {
        if !matches!(
            item.status.as_str(),
            "removed" | "unlisted" | "inaccessible"
        ) {
            continue;
        }
        let Ok(skill_id) = item.id.parse::<i64>() else {
            tracing::warn!(
                id = %item.id,
                status = %item.status,
                "[skill-market] availability response contained invalid Skill ID"
            );
            continue;
        };
        if !requested_ids.contains(&skill_id) {
            tracing::warn!(
                skill_id,
                status = %item.status,
                "[skill-market] availability response contained an unrequested Skill ID"
            );
            continue;
        }
        let mut plugin_in_use = false;
        for (slug, version) in installed_market_skill_plugin_refs(conn, skill_id).await? {
            if crate::plugin_runtime::global::plugin_version_has_active_leases(
                &slug,
                Some(&version),
            )
            .await
            {
                plugin_in_use = true;
                break;
            }
        }
        if plugin_in_use {
            deferred = true;
            tracing::debug!(
                skill_id,
                status = %item.status,
                "[skill-market] deferred unavailable plugin cleanup while in use"
            );
            continue;
        }
        match uninstall_core(conn, skill_id.to_string()).await {
            Ok(()) => tracing::info!(
                skill_id,
                slug = item.slug.as_deref().unwrap_or(""),
                status = %item.status,
                "[skill-market] removed unavailable managed Skill"
            ),
            Err(error) => {
                deferred = true;
                tracing::warn!(
                    skill_id,
                    status = %item.status,
                    error = %error,
                    "[skill-market] unavailable managed Skill cleanup deferred"
                );
            }
        }
    }
    Ok(deferred)
}

async fn installed_market_skill_plugin_refs(
    conn: &DatabaseConnection,
    skill_id: i64,
) -> Result<Vec<(String, String)>, AppCommandError> {
    let mut refs = crate::commands::acp::installed_market_skill_plugin_refs(skill_id);
    if let Some(record) =
        crate::db::service::plugin_installation_service::find_by_market_skill_id(conn, skill_id)
            .await
            .map_err(AppCommandError::db)?
    {
        refs.push((record.installation.slug, record.installation.version));
    }
    refs.sort();
    refs.dedup();
    Ok(refs)
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationTemplateRemote {
    pub id: String,
    pub template_key: String,
    pub title_key: String,
    pub description_key: String,
    pub prompt: String,
    pub trigger_kind: String,
    pub cron: String,
}

pub async fn list_automation_templates(
    conn: &DatabaseConnection,
) -> Result<Vec<AutomationTemplateRemote>, AppCommandError> {
    let builder = client::request(conn, Method::GET, "/automation/templates").await?;
    parse_value(client::send(builder).await?, Some("items"))
}

pub async fn list_core(
    conn: &DatabaseConnection,
    params: SkillMarketListParams,
) -> Result<SkillMarketListResult, AppCommandError> {
    let mut query = vec![("view", params.view)];
    push_query(&mut query, "visibility", params.visibility);
    push_query(&mut query, "publisherType", params.publisher_type);
    push_query(&mut query, "category", params.category);
    push_query(&mut query, "q", params.q);
    push_query(
        &mut query,
        "page",
        params.page.map(|value| value.to_string()),
    );
    push_query(
        &mut query,
        "pageSize",
        params.page_size.map(|value| value.to_string()),
    );
    let builder = client::request(conn, Method::GET, "/skills/list")
        .await?
        .query(&query);
    let mut result: SkillMarketListResult = parse_value(client::send(builder).await?, None)?;
    apply_local_install_versions(conn, &mut result.items).await?;
    spawn_background_availability_reconcile(conn.clone());
    Ok(result)
}

pub async fn categories_core(
    conn: &DatabaseConnection,
) -> Result<Vec<SkillMarketCategory>, AppCommandError> {
    let builder = client::request(conn, Method::GET, "/skills/categories").await?;
    parse_value(client::send(builder).await?, Some("items"))
}

pub async fn detail_core(
    conn: &DatabaseConnection,
    id: String,
    version: Option<String>,
) -> Result<SkillMarketDetail, AppCommandError> {
    let id_number = parse_id(&id)?;
    let body = serde_json::json!({ "id": id_number, "version": version });
    let skill_builder = client::request(conn, Method::POST, "/skills/detail")
        .await?
        .json(&body);
    let mut skill: SkillMarketItem =
        parse_value(client::send(skill_builder).await?, Some("skill"))?;
    apply_local_install_versions(conn, std::slice::from_mut(&mut skill)).await?;
    let files_builder = client::request(conn, Method::POST, "/skills/files")
        .await?
        .json(&body);
    let tree: FileTree = parse_value(client::send(files_builder).await?, None)?;
    let mut files = Vec::new();
    flatten_files(tree.tree, &mut files);
    let install_targets = local_install_targets(conn, id_number).await?;
    Ok(SkillMarketDetail {
        skill,
        files,
        install_targets,
    })
}

pub async fn versions_core(
    conn: &DatabaseConnection,
    id: String,
) -> Result<Vec<SkillMarketVersion>, AppCommandError> {
    let builder = client::request(conn, Method::POST, "/skills/versions/list")
        .await?
        .json(&serde_json::json!({ "id": parse_id(&id)? }));
    let value = client::send(builder).await?;
    if value.get("items").is_some() {
        parse_value(value, Some("items"))
    } else {
        parse_value(value, None)
    }
}

pub async fn publish_core(
    conn: &DatabaseConnection,
    request: SkillMarketPublishRequest,
) -> Result<SkillMarketDetail, AppCommandError> {
    routing_description::validate_routing_descriptions(&request.files, request.package_type)?;
    let dependencies = serialize_dependencies(&request.dependencies)?;
    let form = client::upload_form(
        vec![
            ("slug", request.slug),
            ("displayName", request.display_name),
            ("summary", request.summary),
            ("category", request.category),
            ("iconUrl", request.icon_url.unwrap_or_default()),
            ("visibility", request.visibility),
            ("version", request.version),
            ("changelog", request.changelog),
            (
                "packageType",
                package_type_value(request.package_type).to_string(),
            ),
            ("dependencies", dependencies),
        ],
        &request.tags,
        request.files,
    )?;
    let builder = client::request(conn, Method::POST, "/skills")
        .await?
        .multipart(form)
        .timeout(client::transfer_timeout());
    let skill: SkillMarketItem = parse_value(
        client::send(builder).await.map_err(|error| {
            tracing::error!(error = %error, "[skill-market] publish request failed");
            error
        })?,
        Some("skill"),
    )?;
    tracing::info!(skill_id = %skill.id, slug = %skill.slug, "[skill-market] Skill published");
    Ok(refresh_detail_or_minimal(conn, skill).await)
}

pub async fn add_version_core(
    conn: &DatabaseConnection,
    request: SkillMarketAddVersionRequest,
) -> Result<SkillMarketDetail, AppCommandError> {
    routing_description::validate_routing_descriptions(&request.files, request.package_type)?;
    let id = request.id;
    let dependencies = serialize_dependencies(&request.dependencies)?;
    let mut fallback = detail_core(conn, id.clone(), None).await?;
    let form = client::upload_form(
        vec![
            ("id", parse_id(&id)?.to_string()),
            ("version", request.version),
            ("changelog", request.changelog),
            (
                "packageType",
                package_type_value(request.package_type).to_string(),
            ),
            ("dependencies", dependencies),
        ],
        &[],
        request.files,
    )?;
    let builder = client::request(conn, Method::POST, "/skills/versions")
        .await?
        .multipart(form)
        .timeout(client::transfer_timeout());
    let version: SkillMarketVersion = parse_value(
        client::send(builder).await.map_err(|error| {
            tracing::error!(skill_id = %id, error = %error, "[skill-market] add version request failed");
            error
        })?,
        Some("version"),
    )?;
    fallback.skill.current_version = version;
    fallback.files.clear();
    tracing::info!(skill_id = %id, version = %fallback.skill.current_version.version, "[skill-market] Skill version published");
    Ok(refresh_detail_or_fallback(conn, id, fallback).await)
}

fn serialize_dependencies(
    dependencies: &[SkillDependencyInput],
) -> Result<String, AppCommandError> {
    serde_json::to_string(dependencies).map_err(|error| {
        AppCommandError::invalid_input("Failed to serialize Skill dependencies")
            .with_detail(error.to_string())
    })
}

fn package_type_value(package_type: SkillPackageType) -> &'static str {
    match package_type {
        SkillPackageType::Skill => "skill",
        SkillPackageType::Expert => "expert",
        SkillPackageType::Plugin => "plugin",
    }
}

pub async fn update_metadata_core(
    conn: &DatabaseConnection,
    request: SkillMarketMetadataRequest,
) -> Result<SkillMarketDetail, AppCommandError> {
    let id = request.id.clone();
    let mut value = serde_json::to_value(request).map_err(|error| {
        AppCommandError::invalid_input("Failed to serialize Skill metadata")
            .with_detail(error.to_string())
    })?;
    value["id"] = serde_json::json!(parse_id(&id)?);
    let builder = client::request(conn, Method::POST, "/skills/update")
        .await?
        .json(&value);
    let skill: SkillMarketItem = parse_value(
        client::send(builder).await.map_err(|error| {
            tracing::error!(skill_id = %id, error = %error, "[skill-market] metadata update failed");
            error
        })?,
        Some("skill"),
    )?;
    tracing::info!(skill_id = %id, "[skill-market] Skill metadata updated");
    Ok(refresh_detail_or_minimal(conn, skill).await)
}

pub async fn delete_core(conn: &DatabaseConnection, id: String) -> Result<(), AppCommandError> {
    let builder = client::request(conn, Method::POST, "/skills/delete")
        .await?
        .json(&serde_json::json!({ "id": parse_id(&id)? }));
    client::send(builder).await.map_err(|error| {
        tracing::error!(skill_id = %id, error = %error, "[skill-market] delete request failed");
        error
    })?;
    tracing::info!(skill_id = %id, "[skill-market] Skill deleted");
    Ok(())
}

/// 卸载本地已安装的 market skill（按 `skill_id` 匹配本地 market marker）。
/// 未找到安装记录时幂等成功；被其他已启用 expert 包依赖时拒绝。
pub async fn uninstall_core(conn: &DatabaseConnection, id: String) -> Result<(), AppCommandError> {
    let skill_id = parse_id(&id)?;
    if plugin_install::uninstall_plugin(conn, skill_id).await? {
        tracing::info!(skill_id, "[skill-market] Plugin uninstalled");
    } else {
        let removed =
            crate::commands::acp::uninstall_market_skill_by_id(skill_id).map_err(|error| {
                tracing::error!(
                    skill_id = %skill_id,
                    error = %error,
                    "[skill-market] local uninstall failed"
                );
                AppCommandError::io_error("Failed to uninstall Skill")
                    .with_detail(error.to_string())
            })?;
        if removed == 0 {
            tracing::info!(
                skill_id,
                "[skill-market] no local install found for uninstall"
            );
        } else {
            tracing::info!(skill_id, removed, "[skill-market] Skill uninstalled");
        }
    }
    restore_bundled_skills_after_uninstall(conn, skill_id).await?;
    Ok(())
}

async fn restore_bundled_skills_after_uninstall(
    conn: &DatabaseConnection,
    market_skill_id: i64,
) -> Result<(), AppCommandError> {
    let install = crate::commands::experts::ensure_central_experts_installed().await;
    crate::commands::experts::cleanup_retired_skill_activation_policies(conn, &install.retired)
        .await;
    if !install.errors.is_empty() {
        tracing::error!(
            market_skill_id,
            errors = ?install.errors,
            "[skill-market] bundled Skill restore failed after uninstall"
        );
        return Err(AppCommandError::task_execution_failed(
            "Market Skill was removed but bundled Skill restore failed",
        )
        .with_detail(install.errors.join("\n")));
    }
    if install.installed_count + install.updated_count == 0 {
        return Ok(());
    }
    reconcile_restored_bundled_skills(conn).await?;
    tracing::info!(
        market_skill_id,
        installed = install.installed_count,
        updated = install.updated_count,
        "[skill-market] bundled Skills restored after uninstall"
    );
    Ok(())
}

async fn reconcile_restored_bundled_skills(
    conn: &DatabaseConnection,
) -> Result<(), AppCommandError> {
    for family in [
        crate::commands::managed_skills::ManagedSkillFamily::Experts,
        crate::commands::managed_skills::ManagedSkillFamily::CodexNative,
        crate::commands::managed_skills::ManagedSkillFamily::ComputerUse,
    ] {
        let report =
            crate::commands::managed_skills::reconcile_persisted_family_core(conn, family).await?;
        let failures = report
            .results
            .iter()
            .filter(|result| !result.ok)
            .map(|result| {
                result
                    .error
                    .as_deref()
                    .unwrap_or("Skill publication failed")
            })
            .collect::<Vec<_>>();
        if !failures.is_empty() {
            return Err(AppCommandError::task_execution_failed(
                "Bundled Skill restore is out of sync",
            )
            .with_detail(failures.join("\n")));
        }
    }
    Ok(())
}

/// 重新校验指定版本的制品包并返回最新版本行。服务端制品不可用时
/// 以明确错误返回，供 UI "重建制品" 按钮使用。
pub async fn rebuild_artifact_core(
    conn: &DatabaseConnection,
    id: String,
    version: String,
) -> Result<SkillMarketVersion, AppCommandError> {
    revalidate_artifact_core(conn, &id, &version).await?;
    let requested_version = semver::Version::parse(version.trim())
        .map_err(|error| {
            AppCommandError::invalid_input("Invalid Skill version").with_detail(error.to_string())
        })?
        .to_string();
    let versions = versions_core(conn, id.clone()).await?;
    versions
        .into_iter()
        .find(|value| value.version == requested_version)
        .ok_or_else(|| {
            AppCommandError::not_found(format!(
                "Skill {id} version {requested_version} no longer exists"
            ))
        })
}

async fn refresh_detail_or_minimal(
    conn: &DatabaseConnection,
    skill: SkillMarketItem,
) -> SkillMarketDetail {
    let id = skill.id.clone();
    let fallback = SkillMarketDetail {
        skill,
        files: Vec::new(),
        install_targets: Vec::new(),
    };
    refresh_detail_or_fallback(conn, id, fallback).await
}

async fn refresh_detail_or_fallback(
    conn: &DatabaseConnection,
    id: String,
    fallback: SkillMarketDetail,
) -> SkillMarketDetail {
    match detail_core(conn, id.clone(), None).await {
        Ok(detail) => detail,
        Err(error) => {
            tracing::warn!(skill_id = %id, error = %error, "[skill-market] mutation committed but detail refresh failed");
            fallback
        }
    }
}

fn push_query(query: &mut Vec<(&'static str, String)>, key: &'static str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        query.push((key, value));
    }
}

async fn apply_local_install_versions(
    conn: &DatabaseConnection,
    items: &mut [SkillMarketItem],
) -> Result<(), AppCommandError> {
    let mut installed = crate::commands::acp::installed_market_skill_versions();
    for plugin in crate::db::service::plugin_installation_service::list_installations(conn)
        .await
        .map_err(AppCommandError::db)?
    {
        installed.insert(plugin.market_skill_id, plugin.version);
    }
    for item in items {
        item.installed_version = item
            .id
            .parse::<i64>()
            .ok()
            .and_then(|id| installed.get(&id).cloned());
    }
    Ok(())
}

async fn local_install_targets(
    conn: &DatabaseConnection,
    skill_id: i64,
) -> Result<Vec<crate::models::AgentType>, AppCommandError> {
    let targets = crate::commands::acp::installed_market_skill_targets(skill_id);
    if !targets.is_empty() {
        return Ok(targets);
    }
    let Some(plugin) =
        crate::db::service::plugin_installation_service::find_by_market_skill_id(conn, skill_id)
            .await
            .map_err(AppCommandError::db)?
    else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&plugin.installation.agent_types_json).map_err(|error| {
        AppCommandError::configuration_invalid("Plugin installation targets are invalid")
            .with_detail(error.to_string())
    })
}

fn flatten_files(nodes: Vec<FileNode>, files: &mut Vec<SkillMarketFile>) {
    for node in nodes {
        if node.kind == "file" {
            files.push(SkillMarketFile {
                path: node.path,
                size: node.size.unwrap_or_default(),
                sha256: node.sha256,
                mime_type: node.mime_type,
            });
        } else {
            flatten_files(node.children, files);
        }
    }
}

#[cfg(feature = "tauri-runtime")]
mod tauri_commands;

#[cfg(feature = "tauri-runtime")]
pub use tauri_commands::*;
