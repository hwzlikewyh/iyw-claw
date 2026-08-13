use sea_orm::DatabaseConnection;

use super::capability::{current_arch, current_target, RUNTIME};
use super::client::AgentPlatformClient;
use super::npm_install::{contract_error, version_center_error, ManagedNpmInstallError};
use super::types::{AgentOffer, ResolveAgentRequest};
use crate::acp::error::AcpError;
use crate::acp::registry;
use crate::models::agent::AgentType;

#[derive(Debug, Clone)]
pub(crate) struct ManagedUvxInstall {
    pub(crate) version: String,
    pub(crate) revision: u64,
    pub(crate) effective_policy: String,
    pub(crate) package_spec: String,
    pub(crate) index_url: String,
    pub(crate) source_key: String,
}

pub(crate) async fn resolve_uvx_agent_install(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    current_version: Option<&str>,
    requested_version: &str,
) -> Result<ManagedUvxInstall, ManagedNpmInstallError> {
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
            reason: "manual",
        },
    )
    .await
    .map_err(version_center_error)?;
    install_from_offer(agent_type, offer).map_err(ManagedNpmInstallError::Rejected)
}

pub(crate) async fn confirm_uvx_agent_install(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    current_version: Option<&str>,
    installed: &ManagedUvxInstall,
) -> Result<ManagedUvxInstall, ManagedNpmInstallError> {
    let confirmed =
        resolve_uvx_agent_install(conn, agent_type, current_version, &installed.version).await?;
    let unchanged = confirmed.version == installed.version
        && confirmed.package_spec == installed.package_spec
        && confirmed.index_url == installed.index_url
        && confirmed.source_key == installed.source_key;
    unchanged.then_some(confirmed).ok_or_else(|| {
        ManagedNpmInstallError::Rejected(contract_error(
            "version center uvx offer changed before activation",
        ))
    })
}

fn install_from_offer(
    agent_type: AgentType,
    offer: AgentOffer,
) -> Result<ManagedUvxInstall, AcpError> {
    let registry::AgentDistribution::Uvx { package, .. } =
        registry::get_agent_meta(agent_type).distribution
    else {
        return Err(contract_error("version center Agent is not uvx-based"));
    };
    if offer.delivery.components.len() != 1 {
        return Err(contract_error(
            "version center uvx component set is invalid",
        ));
    }
    let component = &offer.delivery.components[0];
    let package_name = package.split(['[', '=']).next().unwrap_or_default();
    if component.component_key != registry::registry_id_for(agent_type)
        || component.package_name != package_name
        || component.package_version != offer.version
    {
        return Err(contract_error("version center uvx package mismatch"));
    }
    let origin = offer
        .delivery
        .origins
        .iter()
        .find(|origin| origin.source_key == component.source_key)
        .filter(|origin| origin.source_kind == "python_index")
        .ok_or_else(|| contract_error("version center omitted Python index"))?;
    let extras = package
        .strip_prefix(package_name)
        .and_then(|value| value.split("==").next())
        .unwrap_or_default();
    Ok(ManagedUvxInstall {
        version: offer.version.clone(),
        revision: offer.revision,
        effective_policy: offer.effective_update_policy,
        package_spec: format!("{package_name}{extras}=={}", offer.version),
        index_url: managed_python_index(&origin.base_url)?,
        source_key: component.source_key.clone(),
    })
}

fn managed_python_index(value: &str) -> Result<String, AcpError> {
    let parsed = reqwest::Url::parse(value)
        .map_err(|error| contract_error(format!("invalid managed Python index: {error}")))?;
    let local_debug =
        cfg!(debug_assertions) && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost"));
    let unsafe_url = (parsed.scheme() != "https" && !local_debug)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some();
    (!unsafe_url)
        .then(|| value.trim().trim_end_matches('/').to_string())
        .ok_or_else(|| contract_error("version center returned an unsafe Python index"))
}
