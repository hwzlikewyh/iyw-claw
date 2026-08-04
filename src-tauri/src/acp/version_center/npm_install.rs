use base64::Engine;
use sea_orm::DatabaseConnection;

use super::capability::{current_arch, current_target, RUNTIME};
use super::client::AgentPlatformClient;
use super::types::{AgentOffer, ResolveAgentRequest};
use crate::acp::error::AcpError;
use crate::acp::npm_runtime;
use crate::acp::registry;
use crate::app_error::AppErrorCode;
use crate::models::agent::AgentType;

pub(crate) struct ManagedNpmInstall {
    pub(crate) package_name: String,
    pub(crate) install_spec: String,
    pub(crate) version: String,
    pub(crate) registry: String,
    pub(crate) integrity: String,
}

pub(crate) enum ManagedNpmInstallError {
    Unavailable(AcpError),
    Rejected(AcpError),
}

impl ManagedNpmInstallError {
    pub(crate) fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }

    pub(crate) fn into_error(self) -> AcpError {
        match self {
            Self::Unavailable(error) | Self::Rejected(error) => error,
        }
    }
}

pub(crate) async fn resolve_npm_agent_install(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    requested_version: &str,
) -> Result<ManagedNpmInstall, ManagedNpmInstallError> {
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
    install_from_offer(agent_type, offer).map_err(ManagedNpmInstallError::Rejected)
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
    let install_spec = format!("{package_name}@{version}");
    Ok(ManagedNpmInstall {
        package_name,
        install_spec,
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

fn version_center_error(error: crate::app_error::AppCommandError) -> ManagedNpmInstallError {
    let unavailable = error.code == AppErrorCode::NetworkError
        || (error.code == AppErrorCode::InvalidInput
            && matches!(
                error.detail.as_deref(),
                Some(
                    "AGENT_NOT_FOUND"
                        | "AGENT_POLICY_MISSING"
                        | "AGENT_VERSION_NOT_FOUND"
                        | "AGENT_DISTRIBUTION_NOT_FOUND"
                        | "AGENT_DISTRIBUTION_INCOMPLETE"
                        | "AGENT_ARTIFACT_NOT_FOUND"
                        | "AGENT_ARTIFACT_NOT_READY"
                        | "AGENT_STORAGE_UNAVAILABLE"
                        | "AGENT_DOWNLOAD_UNAVAILABLE"
                        | "AGENT_RATE_LIMITED"
                )
            ));
    let detail = error
        .detail
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or(error.message);
    let error = AcpError::DownloadFailed(detail);
    if unavailable {
        ManagedNpmInstallError::Unavailable(error)
    } else {
        ManagedNpmInstallError::Rejected(error)
    }
}

fn contract_error(message: impl Into<String>) -> AcpError {
    AcpError::DownloadFailed(message.into())
}
