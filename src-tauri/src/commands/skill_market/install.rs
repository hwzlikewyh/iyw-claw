use std::collections::BTreeSet;

use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::Method;
use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::commands::acp::MarketSkillInstall;
use crate::models::AgentType;

use super::client;
use super::install_plan::{
    market_marker, validate_downloaded_artifact_size, validate_install_plan, MAX_PACKAGE_BYTES,
};
use super::plugin_install_context::{MarketInstallPlanExecution, PreparedPluginInstall};
use super::plugin_manifest::{validate_plugin_package, ValidatedPluginPackage};
use super::types::{
    parse_id, parse_value, SkillDownloadInfo, SkillInstallPlan, SkillInstallPlanItem,
    SkillPackageType,
};

const PACKAGE_DOWNLOAD_ATTEMPTS: usize = 3;

enum PackageDownloadError {
    Retryable(AppCommandError),
    Permanent(AppCommandError),
}

impl PackageDownloadError {
    fn retryable(error: AppCommandError) -> Self {
        Self::Retryable(error)
    }

    fn permanent(error: AppCommandError) -> Self {
        Self::Permanent(error)
    }

    fn into_error(self) -> AppCommandError {
        match self {
            Self::Retryable(error) | Self::Permanent(error) => error,
        }
    }
}

pub async fn install_core(
    conn: &DatabaseConnection,
    id: String,
    version: String,
    agent_types: Vec<AgentType>,
) -> Result<(), AppCommandError> {
    let requested_id = parse_id(&id)?;
    let requested_version = semver::Version::parse(version.trim())
        .map_err(|error| {
            AppCommandError::invalid_input("Invalid Skill version").with_detail(error.to_string())
        })?
        .to_string();
    let plan = request_install_plan(conn, requested_id, &requested_version).await?;
    validate_install_plan(&plan, requested_id, &requested_version)?;
    let agent_types = validated_agent_targets_for_plan(conn, &plan, agent_types).await?;
    let root_slug = plan.root_slug.clone();
    let package_count = plan.items.len();
    let mut installs = Vec::with_capacity(package_count);
    let mut plugins = Vec::new();
    for item in plan.items {
        let skill_id = item.skill_id.clone();
        let item_version = item.version.clone();
        let package_bytes = download_package(conn, &skill_id, &item_version, &item.download).await.map_err(|error| {
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
        let (package, plugin) = validate_downloaded_package(&package_bytes, &item)?;
        let marker = market_marker(&item, object_hash, requested_id, agent_types.clone())?;
        if let Some(plugin) = plugin {
            plugins.push(PreparedPluginInstall {
                market_skill_id: parse_id(&item.skill_id)?,
                slug: item.slug,
                version: item.version,
                object_sha256: marker.object_sha256.clone(),
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
    validate_install_targets(conn, &requested).await?;
    for agent_type in &requested {
        crate::commands::agent_storage::ensure_active_agent_profile_layout(conn, *agent_type)
            .await?;
    }
    Ok(requested)
}

async fn validate_install_targets(
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
        if setting
            .installed_version
            .as_deref()
            .is_none_or(str::is_empty)
        {
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
    let package_bytes =
        download_package(conn, &root.skill_id, &root.version, &root.download).await?;
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
            if expected.schema_version == 2 {
                super::plugin_signature::verify_v2_plugin_signature(bytes, &item.download)?;
            }
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
        .json(&serde_json::json!({ "id": id, "version": version }));
    parse_value(client::send(builder).await?, None)
}

async fn download_package(
    conn: &DatabaseConnection,
    skill_id: &str,
    version: &str,
    metadata: &SkillDownloadInfo,
) -> Result<Vec<u8>, AppCommandError> {
    let transfer_size = if metadata.artifact_size > 0 {
        metadata.artifact_size
    } else {
        metadata.package_size
    };
    if transfer_size == 0 || transfer_size > MAX_PACKAGE_BYTES {
        return Err(AppCommandError::invalid_input(
            "Skill package size is outside the allowed range",
        ));
    }
    for attempt in 1..=PACKAGE_DOWNLOAD_ATTEMPTS {
        match download_package_once(conn, skill_id, version).await {
            Ok(bytes) => return Ok(bytes),
            Err(PackageDownloadError::Retryable(error)) if attempt < PACKAGE_DOWNLOAD_ATTEMPTS => {
                tracing::warn!(
                    skill_id,
                    version,
                    attempt,
                    max_attempts = PACKAGE_DOWNLOAD_ATTEMPTS,
                    error = %error,
                    error_detail = ?error.detail,
                    "[skill-market] package transfer was incomplete; retrying"
                );
            }
            Err(error) => return Err(error.into_error()),
        }
    }
    unreachable!("package download attempts always return or fail")
}

async fn download_package_once(
    conn: &DatabaseConnection,
    skill_id: &str,
    version: &str,
) -> Result<Vec<u8>, PackageDownloadError> {
    let id = parse_id(skill_id).map_err(PackageDownloadError::permanent)?;
    let response = client::request(conn, Method::POST, "/skills/download")
        .await
        .map_err(PackageDownloadError::permanent)?
        .json(&serde_json::json!({ "id": id, "version": version }))
        .timeout(client::transfer_timeout())
        .send()
        .await
        .map_err(|error| {
            PackageDownloadError::retryable(
                AppCommandError::network("Failed to download Skill package")
                    .with_detail(error.to_string()),
            )
        })?;
    validate_download_response(&response, MAX_PACKAGE_BYTES)
        .map_err(PackageDownloadError::permanent)?;
    read_download(response, MAX_PACKAGE_BYTES).await
}

fn validate_download_response(
    response: &reqwest::Response,
    max_size: u64,
) -> Result<(), AppCommandError> {
    if !response.status().is_success() {
        return Err(
            AppCommandError::network("Skill package download was rejected")
                .with_detail(response.status().to_string()),
        );
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default();
    if !matches!(content_type, "application/zip" | "application/octet-stream") {
        return Err(AppCommandError::network(
            "Skill package response has an unsupported content type",
        ));
    }
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|value| value > max_size)
    {
        return Err(AppCommandError::invalid_input(
            "Skill package exceeds the allowed size",
        ));
    }
    Ok(())
}

async fn read_download(
    response: reqwest::Response,
    max_size: u64,
) -> Result<Vec<u8>, PackageDownloadError> {
    let declared_size = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            PackageDownloadError::retryable(
                AppCommandError::network("Skill download was interrupted")
                    .with_detail(error.to_string()),
            )
        })?;
        if bytes.len().saturating_add(chunk.len()) as u64 > max_size {
            return Err(PackageDownloadError::permanent(
                AppCommandError::invalid_input("Skill package exceeds the allowed size"),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    if declared_size.is_some_and(|value| bytes.len() as u64 != value) {
        return Err(PackageDownloadError::retryable(
            AppCommandError::network("Skill package is incomplete").with_detail(format!(
                "declared_size={}, received_size={}",
                declared_size.unwrap_or_default(),
                bytes.len()
            )),
        ));
    }
    Ok(bytes)
}
