use sea_orm::DatabaseConnection;
use semver::Version;

use crate::acp::registry::{self, AgentDistribution};
use crate::acp::version_center::types::{AgentOffer, CatalogSnapshot, ToolOffer, ToolRequirement};

pub const CATALOG_SCHEMA_VERSION: u32 = 1;
pub const RUNTIME: &str = "desktop";
pub const TARGET: &str = "windows";
pub const ARCH: &str = "x86_64";

pub const TOOL_IDS: [&str; 3] = ["git", "node", "uv"];

pub fn known_tool(tool_id: &str) -> bool {
    TOOL_IDS.contains(&tool_id)
}

pub fn current_target() -> &'static str {
    if cfg!(windows) {
        TARGET
    } else {
        std::env::consts::OS
    }
}

pub fn current_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => ARCH,
        "aarch64" => "aarch64",
        "x86" => "x86",
        other => other,
    }
}

pub fn validate_catalog(snapshot: &CatalogSnapshot) -> Result<(), String> {
    if snapshot.schema_version != CATALOG_SCHEMA_VERSION {
        return Err("unsupported Agent catalog schema".to_string());
    }
    for platform in &snapshot.platforms {
        if registry::from_registry_id(&platform.registry_id).is_none() {
            continue;
        }
        if !platform.id.is_empty() && !valid_platform_id(&platform.id) {
            return Err("invalid Agent platform id".to_string());
        }
        if !matches!(platform.status.as_str(), "active" | "hidden" | "disabled") {
            return Err("invalid Agent platform status".to_string());
        }
        validate_catalog_version(&platform.recommended_version)?;
        validate_catalog_version(&platform.minimum_safe_version)?;
    }
    for tool in &snapshot.tools {
        if !known_tool(&tool.tool_id) {
            continue;
        }
        validate_catalog_version(&tool.recommended_version)?;
        validate_catalog_version(&tool.minimum_safe_version)?;
    }
    Ok(())
}

fn valid_platform_id(value: &str) -> bool {
    !value.is_empty() && !value.starts_with('0') && value.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn validate_agent_offer(offer: &AgentOffer) -> Result<(), String> {
    let agent = registry::from_registry_id(&offer.registry_id)
        .ok_or_else(|| "unknown Agent registry id".to_string())?;
    validate_version(&offer.version)?;
    validate_delivery_dimensions(
        &offer.delivery.runtime,
        &offer.delivery.target,
        &offer.delivery.arch,
        offer.delivery.recipe_schema_version,
    )?;
    let meta = registry::get_agent_meta(agent);
    crate::acp::deepseek_config::validate_tool_version(agent, &offer.version)?;
    if offer
        .delivery
        .artifact_id
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err("Agent artifact id is empty".to_string());
    }
    match (&meta.distribution, offer.delivery.kind.as_str()) {
        (AgentDistribution::Binary { .. }, "binary") => require_artifact(offer)?,
        (AgentDistribution::Npx { package, .. }, "npm") => {
            validate_npm_components(package, &offer.delivery.components)?;
            validate_trusted_node_requirement(agent, &offer.delivery.node_required)?;
        }
        (AgentDistribution::Uvx { package, .. }, "uvx") => {
            validate_uvx_components(package, &offer.delivery.components)?
        }
        _ => return Err("delivery kind is not allowed for this Agent".to_string()),
    }
    for requirement in &offer.delivery.tool_requirements {
        validate_tool_requirement(requirement)?;
    }
    Ok(())
}

fn validate_trusted_node_requirement(
    agent: crate::models::agent::AgentType,
    offered: &str,
) -> Result<(), String> {
    let Some(required) = crate::acp::trusted_agents::minimum_node_version(agent) else {
        return Ok(());
    };
    let offered = Version::parse(offered.trim())
        .map_err(|_| "trusted npm Agent offer has an invalid Node.js requirement".to_string())?;
    let required = Version::parse(required)
        .map_err(|_| "trusted npm Agent Node.js requirement is invalid".to_string())?;
    (offered >= required)
        .then_some(())
        .ok_or_else(|| "Agent offer Node.js requirement is below the trusted minimum".to_string())
}

pub fn validate_tool_offer(offer: &ToolOffer) -> Result<(), String> {
    if !known_tool(&offer.tool_id) {
        return Err("unknown managed tool".to_string());
    }
    validate_version(&offer.version)?;
    validate_delivery_dimensions(
        &offer.artifact.runtime,
        &offer.artifact.target,
        &offer.artifact.arch,
        CATALOG_SCHEMA_VERSION,
    )?;
    if offer.artifact.package_kind != "zip" || offer.artifact.size <= 0 {
        return Err("managed tool artifact is not an approved ZIP".to_string());
    }
    validate_sha256(&offer.artifact.sha256)
}

/// 已安装的 DeepSeek Harness 不允许通过通用 Node offer 降级到可信启动下限以下。
pub(crate) async fn validate_node_offer_for_active_deepseek(
    conn: &DatabaseConnection,
    offer: &ToolOffer,
) -> Result<(), String> {
    if offer.tool_id != "node" || !active_deepseek_installation(conn).await? {
        return Ok(());
    }
    let required =
        crate::acp::trusted_agents::minimum_node_version(crate::models::agent::AgentType::DeepSeek)
            .ok_or_else(|| "DeepSeek Harness Node.js requirement is unavailable".to_string())?;
    let offered = Version::parse(&offer.version)
        .map_err(|_| "managed Node.js offer has an invalid version".to_string())?;
    let required = Version::parse(required)
        .map_err(|_| "DeepSeek Harness Node.js requirement is invalid".to_string())?;
    if offered < required {
        tracing::warn!(
            offered_node = %offered,
            required_node = %required,
            "[agent-version-center] Node.js offer rejected for active DeepSeek Harness"
        );
        return Err(
            "managed Node.js offer is below the active DeepSeek Harness requirement".into(),
        );
    }
    Ok(())
}

async fn active_deepseek_installation(conn: &DatabaseConnection) -> Result<bool, String> {
    crate::acp::version_center::inventory::list_agent_installations(
        conn,
        crate::models::agent::AgentType::DeepSeek,
    )
    .await
    .map_err(|error| format!("failed to inspect DeepSeek Harness installation: {error}"))
    .map(|installations| {
        installations.into_iter().any(|installation| {
            installation.status == crate::acp::version_center::inventory::STATUS_ACTIVE
                && installation.verified
                && installation.platform == crate::acp::registry::current_platform()
        })
    })
}

pub fn validate_tool_requirement(requirement: &ToolRequirement) -> Result<(), String> {
    if !known_tool(&requirement.tool_id) {
        return Err("Agent requires an unknown managed tool".to_string());
    }
    validate_version(&requirement.minimum_version)?;
    if !requirement.maximum_version.is_empty() {
        validate_version(&requirement.maximum_version)?;
    }
    Ok(())
}

fn validate_catalog_version(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Ok(());
    }
    validate_version(value)
}

fn validate_version(value: &str) -> Result<(), String> {
    Version::parse(value.trim())
        .map(|_| ())
        .map_err(|_| "invalid release version".to_string())
}

fn validate_delivery_dimensions(
    runtime: &str,
    target: &str,
    arch: &str,
    schema_version: u32,
) -> Result<(), String> {
    if runtime != RUNTIME || target != current_target() || arch != current_arch() {
        return Err("release dimensions do not match this client".to_string());
    }
    if schema_version != CATALOG_SCHEMA_VERSION {
        return Err("unsupported delivery recipe schema".to_string());
    }
    Ok(())
}

fn require_artifact(offer: &AgentOffer) -> Result<(), String> {
    offer
        .delivery
        .artifact_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .map(|_| ())
        .ok_or_else(|| "binary Agent release has no artifact".to_string())
}

fn validate_npm_components(
    package: &str,
    components: &[crate::acp::version_center::types::DeliveryComponent],
) -> Result<(), String> {
    let package_name = npm_package_name(package);
    let primary = components
        .iter()
        .filter(|component| component.package_name == package_name)
        .count();
    let secondary = components
        .iter()
        .filter(|component| component.component_key == "pi-coding-agent")
        .all(|component| component.package_name == "@earendil-works/pi-coding-agent");
    let expected_len = if package_name == "pi-acp" { 2 } else { 1 };
    (primary == 1 && secondary && components.len() == expected_len)
        .then_some(())
        .ok_or_else(|| "npm component is outside the compiled allowlist".to_string())
}

fn npm_package_name(spec: &str) -> &str {
    let spec = spec.trim();
    let version_separator = if spec.starts_with('@') {
        spec.find('/')
            .and_then(|slash| spec[slash + 1..].find('@').map(|index| slash + 1 + index))
    } else {
        spec.find('@')
    };
    version_separator.map_or(spec, |index| &spec[..index])
}

fn validate_uvx_components(
    package: &str,
    components: &[crate::acp::version_center::types::DeliveryComponent],
) -> Result<(), String> {
    let package_name = package.split(['[', '=']).next().unwrap_or_default();
    let matches = components
        .iter()
        .filter(|component| component.package_name == package_name)
        .count();
    (matches == 1 && components.len() == 1)
        .then_some(())
        .ok_or_else(|| "uvx component is outside the compiled allowlist".to_string())
}

fn validate_sha256(value: &str) -> Result<(), String> {
    let valid = value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    valid
        .then_some(())
        .ok_or_else(|| "invalid artifact SHA-256".to_string())
}
