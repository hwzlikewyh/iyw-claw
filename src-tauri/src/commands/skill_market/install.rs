use std::collections::BTreeSet;

use reqwest::Method;
use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::commands::acp::MarketSkillInstall;
use crate::models::AgentType;

use super::client;
use super::install_plan::{
    market_marker, validate_downloaded_artifact_size, validate_install_plan,
};
use super::plugin_install_context::{MarketInstallPlanExecution, PreparedPluginInstall};
use super::plugin_manifest::{validate_plugin_package, ValidatedPluginPackage};
use super::types::{
    parse_id, parse_value, SkillInstallPlan, SkillInstallPlanItem, SkillPackageType,
};

pub async fn install_core(
    conn: &DatabaseConnection,
    id: String,
    version: String,
    agent_types: Vec<AgentType>,
) -> Result<(), AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let account_scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
    let requested_id = parse_id(&id)?;
    let requested_version = semver::Version::parse(version.trim())
        .map_err(|error| {
            AppCommandError::invalid_input("Invalid Skill version").with_detail(error.to_string())
        })?
        .to_string();
    let plan = request_install_plan(conn, requested_id, &requested_version).await?;
    validate_install_plan(&plan, requested_id, &requested_version)?;
    for item in &plan.items {
        if let Some(plugin) = &item.plugin {
            super::plugin_platforms::require_current(&plugin.platforms)?;
        }
    }
    let mut managed_office_slugs = BTreeSet::new();
    for item in &plan.items {
        if item.slug != plan.root_slug
            && crate::commands::office_tools::is_officecli_skill_id(&item.slug)
        {
            crate::commands::office_tools::ensure_managed_office_skill_ready(&item.slug)
                .await
                .map_err(|error| {
                    AppCommandError::task_execution_failed(
                        "OfficeCLI dependency is not ready for this Skill",
                    )
                    .with_detail(error.to_string())
                })?;
            managed_office_slugs.insert(item.slug.clone());
        }
    }
    let agent_types = validated_agent_targets_for_plan(conn, &plan, agent_types).await?;
    let root_slug = plan.root_slug.clone();
    let package_count = plan.items.len().saturating_sub(managed_office_slugs.len());
    let mut installs = Vec::with_capacity(package_count);
    let mut plugins = Vec::new();
    for item in plan.items {
        if managed_office_slugs.contains(&item.slug) {
            continue;
        }
        let skill_id = item.skill_id.clone();
        let item_version = item.version.clone();
        let package_bytes = super::download_ticket::download_package(conn, &item).await.map_err(|error| {
            tracing::error!(skill_id = %skill_id, slug = %item.slug, version = %item_version, error = %error, "[skill-market] package download failed");
            error
        })?;
        validate_downloaded_artifact_size(&item, package_bytes.len())?;
        let object_hash = crate::acp::skill_package::hash_bytes(&package_bytes);
        if !item.download.object_sha256.trim().is_empty()
            && !object_hash.eq_ignore_ascii_case(&item.download.object_sha256)
        {
            return Err(AppCommandError::invalid_input(format!(
                "Downloaded Skill package integrity check failed for {}@{}",
                item.slug, item.version
            )));
        }
        tracing::debug!(
            skill_id = %skill_id,
            version = %item_version,
            bytes = package_bytes.len(),
            declared_artifact_size = item.download.artifact_size,
            declared_package_size = item.download.package_size,
            object_sha256_present = !item.download.object_sha256.trim().is_empty(),
            "[skill-market] package transfer completed"
        );
        let (package, plugin) = validate_downloaded_package(&package_bytes, &item).map_err(|error| {
            tracing::error!(skill_id = %skill_id, slug = %item.slug, version = %item_version, error = %error, "[skill-market] package validation failed");
            error
        })?;
        let marker = market_marker(&item, object_hash, requested_id, agent_types.clone())?;
        if let Some(plugin) = plugin {
            plugins.push(PreparedPluginInstall {
                market_skill_id: parse_id(&item.skill_id)?,
                slug: item.slug,
                version: item.version,
                object_sha256: marker.object_sha256.clone(),
                publisher_id: format!("fusion:{}", item.publisher_type),
                signature_key_id: item.download.signature_key_id.clone(),
                package,
                plugin,
                marker,
            });
        } else {
            installs.push(MarketSkillInstall { marker, package });
        }
    }
    super::plugin_install::install_market_plan(MarketInstallPlanExecution {
        conn,
        account_scope,
        agent_types: &agent_types,
        root_skill_id: requested_id,
        skill_installs: installs,
        plugins,
    })
    .await
    .map_err(|error| {
        tracing::error!(skill_id = %id, version = %requested_version, agent_types = ?agent_types, error = %error, "[skill-market] local installation failed");
        error
    })?;
    tracing::info!(
        skill_id = %id,
        slug = %root_slug,
        version = %requested_version,
        packages = package_count,
        agent_types = ?agent_types,
        "[skill-market] expert package dependency closure installed"
    );
    Ok(())
}

async fn validated_agent_targets_for_plan(
    conn: &DatabaseConnection,
    plan: &SkillInstallPlan,
    requested: Vec<AgentType>,
) -> Result<Vec<AgentType>, AppCommandError> {
    let requires_skills = plan.items.iter().any(|item| {
        item.package_type != SkillPackageType::Plugin
            || item.plugin.as_ref().is_some_and(|plugin| {
                plugin
                    .components
                    .iter()
                    .any(|component| component.kind == "skill")
            })
    });
    if !requires_skills {
        return Ok(Vec::new());
    }
    validate_install_targets(conn, &requested)
        .await
        .map_err(|error| {
            tracing::warn!(
                skill_id = %plan.root_skill_id,
                version = %plan.root_version,
                agent_types = ?requested,
                error_code = ?error.code,
                error = %error,
                "[skill-market] install target validation failed"
            );
            error
        })?;
    for agent_type in &requested {
        crate::commands::agent_storage::ensure_active_agent_profile_layout(conn, *agent_type)
            .await?;
    }
    Ok(requested)
}

pub(super) async fn validate_install_targets(
    conn: &DatabaseConnection,
    agent_types: &[AgentType],
) -> Result<(), AppCommandError> {
    if agent_types.is_empty() {
        return Err(AppCommandError::invalid_input(
            "Select at least one installed Agent",
        ));
    }
    let mut unique = BTreeSet::new();
    for agent_type in agent_types {
        if !unique.insert(*agent_type) {
            return Err(AppCommandError::invalid_input(
                "Skill install targets contain duplicate Agents",
            ));
        }
        if crate::commands::acp::skill_storage_spec(*agent_type).is_none() {
            return Err(AppCommandError::invalid_input(format!(
                "{agent_type} does not support Skills"
            )));
        }
        let setting =
            crate::db::service::agent_setting_service::get_by_agent_type(conn, *agent_type)
                .await?
                .ok_or_else(|| {
                    AppCommandError::invalid_input(format!("{agent_type} is not installed"))
                })?;
        // 内置星河随应用分发，安装状态不能依赖旧 npm 版本记录。
        if !crate::internal_xinghe_worker::installation_available(
            *agent_type,
            setting.installed_version.as_deref(),
        ) {
            return Err(AppCommandError::invalid_input(format!(
                "{agent_type} is not installed"
            )));
        }
        if !setting.enabled {
            return Err(AppCommandError::invalid_input(format!(
                "{agent_type} is disabled"
            )));
        }
    }
    Ok(())
}

/// 重新校验指定版本的制品包：拉取安装计划并下载根包做完整性校验，
/// 不落盘、不执行本地安装。供 `skill_market_rebuild_artifact` 使用；
/// 服务端制品仍在构建或已损坏时以明确错误返回。
pub async fn revalidate_artifact_core(
    conn: &DatabaseConnection,
    id: &str,
    version: &str,
) -> Result<(), AppCommandError> {
    let requested_id = parse_id(id)?;
    let requested_version = semver::Version::parse(version.trim())
        .map_err(|error| {
            AppCommandError::invalid_input("Invalid Skill version").with_detail(error.to_string())
        })?
        .to_string();
    let plan = request_install_plan(conn, requested_id, &requested_version).await?;
    validate_install_plan(&plan, requested_id, &requested_version)?;
    let root = plan.items.last().expect("validated non-empty install plan");
    let package_bytes = super::download_ticket::download_package(conn, root).await?;
    validate_downloaded_artifact_size(root, package_bytes.len())?;
    let object_hash = crate::acp::skill_package::hash_bytes(&package_bytes);
    if !root.download.object_sha256.trim().is_empty()
        && !object_hash.eq_ignore_ascii_case(&root.download.object_sha256)
    {
        return Err(AppCommandError::invalid_input(format!(
            "Rebuilt Skill artifact integrity check failed for {}@{}",
            root.slug, root.version
        )));
    }
    validate_downloaded_package(&package_bytes, root)?;
    tracing::info!(
        skill_id = %id,
        version = %requested_version,
        "[skill-market] artifact re-validation passed"
    );
    Ok(())
}

fn validate_downloaded_package(
    bytes: &[u8],
    item: &SkillInstallPlanItem,
) -> Result<
    (
        crate::acp::skill_package::ValidatedSkillPackage,
        Option<ValidatedPluginPackage>,
    ),
    AppCommandError,
> {
    match item.package_type {
        SkillPackageType::Skill | SkillPackageType::Expert => {
            let package =
                crate::acp::skill_package::validate_zip(bytes, &item.download.content_sha256)?;
            Ok((package, None))
        }
        SkillPackageType::Plugin => {
            let expected = item.plugin.as_ref().ok_or_else(|| {
                AppCommandError::configuration_invalid(
                    "Plugin install plan is missing component metadata",
                )
            })?;
            let package = crate::acp::skill_package::validate_plugin_zip(
                bytes,
                &item.download.content_sha256,
            )?;
            let plugin = validate_plugin_package(&package, expected, &item.slug, &item.version)?;
            Ok((package, Some(plugin)))
        }
    }
}

pub(super) fn map_local_install_error(error: crate::acp::error::AcpError) -> AppCommandError {
    let detail = error.to_string();
    let lowered = detail.to_ascii_lowercase();
    let result = if lowered.contains("already exists")
        || lowered.contains("different market skill")
        || lowered.contains("uses this skill slug")
        || lowered.contains("will not be overwritten")
    {
        AppCommandError::already_exists("A local Skill already uses this slug")
    } else if lowered.contains("storage") && lowered.contains("not initialized") {
        AppCommandError::agent_storage_not_initialized("Agent storage is not initialized")
    } else if lowered.contains("not supported") {
        AppCommandError::invalid_input("The selected Agent cannot install Skills")
    } else {
        AppCommandError::io_error("Failed to install Skill package")
    };
    result.with_detail(detail)
}

async fn request_install_plan(
    conn: &DatabaseConnection,
    id: i64,
    version: &str,
) -> Result<SkillInstallPlan, AppCommandError> {
    let builder = client::request(conn, Method::POST, "/skills/install-plan")
        .await?
        .json(&serde_json::json!({ "id": id.to_string(), "version": version, "client": super::plugin_client_version::context() }));
    parse_value(client::send(builder).await?, None)
}
