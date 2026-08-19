use base64::Engine;
use sea_orm::DatabaseConnection;

use super::capability::{current_arch, current_target, RUNTIME};
use super::client::AgentPlatformClient;
use super::types::{AgentOffer, ResolveAgentRequest};
use crate::acp::error::AcpError;
use crate::acp::npm_runtime;
use crate::acp::registry;
use crate::acp::version_center::fallback::{self, AgentFallbackReason};
use crate::acp::version_center::inventory::{self, is_verified_origin, STATUS_ACTIVE};
use crate::models::agent::AgentType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedNpmPackage {
    pub(crate) component_key: String,
    pub(crate) package_name: String,
    pub(crate) install_spec: String,
    pub(crate) package_version: String,
    pub(crate) registry: String,
    pub(crate) integrity: String,
    pub(crate) source_key: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ManagedNpmInstall {
    pub(crate) version: String,
    pub(crate) revision: u64,
    pub(crate) effective_policy: String,
    pub(crate) packages: Vec<ManagedNpmPackage>,
    pub(crate) bundle_offer: Option<AgentOffer>,
}

pub(crate) enum ManagedNpmInstallError {
    Unavailable(AcpError),
    PolicyMissing(AcpError),
    Rejected(AcpError),
}

impl ManagedNpmInstallError {
    pub(crate) fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable(_) | Self::PolicyMissing(_))
    }

    pub(crate) fn is_policy_missing(&self) -> bool {
        matches!(self, Self::PolicyMissing(_))
    }
}

impl ManagedNpmInstallError {
    pub(crate) fn into_error(self) -> AcpError {
        match self {
            Self::Unavailable(error) | Self::PolicyMissing(error) | Self::Rejected(error) => error,
        }
    }
}

pub(crate) async fn resolve_npm_agent_install(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    current_version: Option<&str>,
    requested_version: &str,
    reason: &str,
) -> Result<ManagedNpmInstall, ManagedNpmInstallError> {
    let preferences = crate::update::preferences::load(conn)
        .await
        .map_err(version_center_error)?;
    let offer = AgentPlatformClient::resolve_agent(
        conn,
        ResolveAgentRequest {
            registry_id: registry::registry_id_for(agent_type),
            current_version: current_version.unwrap_or_default(),
            requested_version: Some(requested_version),
            pinned_version: None,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: current_target(),
            arch: current_arch(),
            channel: preferences.channel.as_str(),
            reason,
        },
    )
    .await
    .map_err(version_center_error)?;
    install_from_offer(agent_type, offer).map_err(ManagedNpmInstallError::Rejected)
}

pub(crate) async fn confirm_npm_agent_install(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    current_version: Option<&str>,
    installed: &ManagedNpmInstall,
    reason: &str,
) -> Result<ManagedNpmInstall, ManagedNpmInstallError> {
    let confirmed = resolve_npm_agent_install(
        conn,
        agent_type,
        current_version,
        &installed.version,
        reason,
    )
    .await?;
    if confirmed.version != installed.version
        || confirmed.packages != installed.packages
        || bundle_identity(confirmed.bundle_offer.as_ref())
            != bundle_identity(installed.bundle_offer.as_ref())
    {
        return Err(ManagedNpmInstallError::Rejected(contract_error(
            "version center npm offer changed before activation",
        )));
    }
    Ok(confirmed)
}

/// 安装前校验 npm 实际使用的 Node；受管运行时还必须与已验证库存一致。
pub(crate) async fn ensure_npm_node_requirement(
    conn: &DatabaseConnection,
    agent_type: AgentType,
) -> Result<(), AcpError> {
    let Some(required) = crate::acp::trusted_agents::minimum_node_version(agent_type) else {
        return Ok(());
    };
    let required = semver::Version::parse(required)
        .map_err(|_| contract_error("trusted Agent Node.js requirement is invalid"))?;
    let active_version = inventory::list_tool_settings(conn)
        .await?
        .into_iter()
        .find(|setting| setting.tool_id == "node")
        .and_then(|setting| setting.active_version);
    if let Some(active_version) = active_version {
        validate_managed_node_inventory(conn, agent_type, &required, &active_version).await?;
    }
    let environment = std::collections::BTreeMap::new();
    let required_text = required.to_string();
    crate::acp::preflight::enforce_minimum_node_version(&environment, &required_text)
        .await
        .map_err(|detail| node_path_requirement_error(agent_type, &required, &detail))
}

async fn validate_managed_node_inventory(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    required: &semver::Version,
    active_version: &str,
) -> Result<(), AcpError> {
    let active = inventory::list_tool_installations(conn, "node")
        .await?
        .into_iter()
        .find(|installation| {
            installation.version == active_version
                && installation.status == STATUS_ACTIVE
                && installation.verified
                && is_verified_origin(&installation.origin)
        })
        .ok_or_else(|| managed_node_requirement_error(agent_type, required, active_version))?;
    let installed = semver::Version::parse(&active.version)
        .map_err(|_| managed_node_requirement_error(agent_type, required, active_version))?;
    if installed >= *required {
        return Ok(());
    }
    tracing::warn!(
        agent = registry::registry_id_for(agent_type),
        installed_node = %installed,
        required_node = %required,
        "[agent-version-center] npx Agent install blocked by managed Node.js requirement"
    );
    Err(managed_node_requirement_error(
        agent_type,
        required,
        active_version,
    ))
}

fn managed_node_requirement_error(
    agent_type: AgentType,
    required: &semver::Version,
    installed: &str,
) -> AcpError {
    AcpError::DownloadFailed(format!(
        "{} requires Node.js >= {required}; active managed Node.js is {installed}",
        registry::get_agent_meta(agent_type).name,
    ))
}

fn node_path_requirement_error(
    agent_type: AgentType,
    required: &semver::Version,
    detail: &str,
) -> AcpError {
    AcpError::DownloadFailed(format!(
        "{} requires Node.js >= {required}: {detail}",
        registry::get_agent_meta(agent_type).name,
    ))
}

fn install_from_offer(
    agent_type: AgentType,
    offer: AgentOffer,
) -> Result<ManagedNpmInstall, AcpError> {
    let bundle_offer = offer.delivery.artifact_id.as_ref().map(|_| offer.clone());
    let allowed = expected_npm_packages(agent_type)?;
    if offer.delivery.components.len() != allowed.len() {
        return Err(contract_error(
            "version center npm component set is incomplete",
        ));
    }
    let mut packages = Vec::with_capacity(allowed.len());
    for (component_key, package_name) in allowed {
        let component = offer
            .delivery
            .components
            .iter()
            .find(|component| {
                component.component_key == component_key && component.package_name == package_name
            })
            .ok_or_else(|| contract_error("version center omitted an allowlisted npm component"))?;
        let origin = offer
            .delivery
            .origins
            .iter()
            .find(|origin| origin.source_key == component.source_key)
            .filter(|origin| origin.source_kind == "npm_registry")
            .ok_or_else(|| contract_error("version center omitted npm origin"))?;
        packages.push(ManagedNpmPackage {
            component_key: component_key.to_string(),
            package_name: package_name.to_string(),
            install_spec: format!("{package_name}@{}", component.package_version),
            package_version: component.package_version.clone(),
            registry: managed_registry(&origin.base_url)?,
            integrity: managed_integrity(&component.registry_integrity)?,
            source_key: component.source_key.clone(),
        });
    }
    if packages
        .windows(2)
        .any(|pair| pair[0].registry != pair[1].registry)
    {
        return Err(contract_error(
            "version center npm components use different registries",
        ));
    }
    let primary = packages
        .first()
        .ok_or_else(|| contract_error("version center omitted npm package"))?;
    let primary_version = offer
        .delivery
        .components
        .iter()
        .find(|component| component.component_key == primary.component_key)
        .map(|component| component.package_version.as_str())
        .unwrap_or_default();
    if semver::Version::parse(primary_version).is_err() || primary_version != offer.version {
        return Err(contract_error("version center npm version mismatch"));
    }
    Ok(ManagedNpmInstall {
        version: offer.version,
        revision: offer.revision,
        effective_policy: offer.effective_update_policy,
        packages,
        bundle_offer,
    })
}

fn bundle_identity(offer: Option<&AgentOffer>) -> Option<(&str, &str)> {
    let offer = offer?;
    Some((
        offer.version_id.as_str(),
        offer.delivery.artifact_id.as_deref()?,
    ))
}

fn expected_npm_packages(agent_type: AgentType) -> Result<Vec<(&'static str, String)>, AcpError> {
    let registry::AgentDistribution::Npx { package, .. } =
        registry::get_agent_meta(agent_type).distribution
    else {
        return Err(contract_error("version center Agent is not npm-based"));
    };
    let index = package
        .rfind('@')
        .filter(|index| *index > 0)
        .unwrap_or(package.len());
    let mut packages = vec![(
        registry::registry_id_for(agent_type),
        package[..index].to_string(),
    )];
    if agent_type == AgentType::Pi {
        packages.push((
            "pi-coding-agent",
            "@earendil-works/pi-coding-agent".to_string(),
        ));
    }
    Ok(packages)
}

fn managed_registry(value: &str) -> Result<String, AcpError> {
    let registry = npm_runtime::npm_registry(Some(value))?;
    let parsed = reqwest::Url::parse(&registry)
        .map_err(|error| contract_error(format!("invalid managed registry: {error}")))?;
    let local_debug =
        cfg!(debug_assertions) && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost"));
    let unsafe_url = (parsed.scheme() != "https" && !local_debug)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some();
    (!unsafe_url)
        .then_some(registry)
        .ok_or_else(|| contract_error("version center returned an unsafe npm origin"))
}

fn managed_integrity(value: &str) -> Result<String, AcpError> {
    let value = value.trim();
    let (algorithm, digest) = value
        .split_once('-')
        .ok_or_else(|| contract_error("version center npm integrity is invalid"))?;
    if !matches!(algorithm, "sha256" | "sha384" | "sha512") {
        return Err(contract_error(
            "version center npm integrity algorithm is unsupported",
        ));
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(digest)
        .map_err(|_| contract_error("version center npm integrity is invalid"))?;
    let expected_len = match algorithm {
        "sha256" => 32,
        "sha384" => 48,
        "sha512" => 64,
        _ => unreachable!(),
    };
    if decoded.len() != expected_len {
        return Err(contract_error(
            "version center npm integrity digest length is invalid",
        ));
    }
    Ok(value.to_string())
}

pub(super) fn version_center_error(
    error: crate::app_error::AppCommandError,
) -> ManagedNpmInstallError {
    let fallback_reason = fallback::classify(&error);
    let detail = error
        .detail
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or(error.message);
    let error = AcpError::DownloadFailed(detail);
    if fallback_reason == Some(AgentFallbackReason::PolicyMissing) {
        ManagedNpmInstallError::PolicyMissing(error)
    } else if fallback_reason.is_some() {
        ManagedNpmInstallError::Unavailable(error)
    } else {
        ManagedNpmInstallError::Rejected(error)
    }
}

pub(super) fn contract_error(message: impl Into<String>) -> AcpError {
    AcpError::DownloadFailed(message.into())
}
