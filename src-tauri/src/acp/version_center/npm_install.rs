use base64::Engine;
use sea_orm::DatabaseConnection;

use super::capability::{current_arch, current_target, RUNTIME};
use super::client::AgentPlatformClient;
use super::types::{AgentOffer, ResolveAgentRequest};
use crate::acp::error::AcpError;
use crate::acp::npm_runtime;
use crate::acp::registry;
use crate::models::agent::AgentType;

pub(crate) struct ManagedNpmInstall {
    pub(crate) install_spec: String,
    pub(crate) version: String,
    pub(crate) registry: String,
    pub(crate) integrity: String,
}

pub(crate) async fn resolve_npm_agent_install(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    requested_version: &str,
) -> Result<ManagedNpmInstall, AcpError> {
    let preferences = crate::update::preferences::load(conn)
        .await
        .map_err(version_center_error)?;
    let offer = AgentPlatformClient::resolve_agent(
        conn,
        ResolveAgentRequest {
            registry_id: registry::registry_id_for(agent_type),
            current_version: "",
            requested_version: Some(requested_version),
            pinned_version: None,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: current_target(),
            arch: current_arch(),
            channel: preferences.channel.as_str(),
            reason: "manual",
        },
    )
    .await
    .map_err(version_center_error)?;
    install_from_offer(agent_type, offer)
}

fn install_from_offer(
    agent_type: AgentType,
    offer: AgentOffer,
) -> Result<ManagedNpmInstall, AcpError> {
    let package_name = expected_package_name(agent_type)?;
    let component = offer
        .delivery
        .components
        .iter()
        .find(|component| component.package_name == package_name)
        .ok_or_else(|| contract_error("version center omitted npm package"))?;
    let version = component.package_version.clone();
    if semver::Version::parse(&version).is_err() || version != offer.version {
        return Err(contract_error("version center npm version mismatch"));
    }
    let origin = offer
        .delivery
        .origins
        .iter()
        .find(|origin| origin.source_key == component.source_key)
        .filter(|origin| origin.source_kind == "npm_registry")
        .ok_or_else(|| contract_error("version center omitted npm origin"))?;
    let registry = managed_registry(&origin.base_url)?;
    let integrity = managed_integrity(&component.registry_integrity)?;
    Ok(ManagedNpmInstall {
        install_spec: format!("{package_name}@{version}"),
        version,
        registry,
        integrity,
    })
}

fn expected_package_name(agent_type: AgentType) -> Result<String, AcpError> {
    let registry::AgentDistribution::Npx { package, .. } =
        registry::get_agent_meta(agent_type).distribution
    else {
        return Err(contract_error("version center Agent is not npm-based"));
    };
    let index = package
        .rfind('@')
        .filter(|index| *index > 0)
        .unwrap_or(package.len());
    Ok(package[..index].to_string())
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
    if decoded.is_empty() {
        return Err(contract_error("version center npm integrity is empty"));
    }
    Ok(value.to_string())
}

fn version_center_error(error: crate::app_error::AppCommandError) -> AcpError {
    let detail = error
        .detail
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or(error.message);
    AcpError::DownloadFailed(detail)
}

fn contract_error(message: impl Into<String>) -> AcpError {
    AcpError::DownloadFailed(message.into())
}
