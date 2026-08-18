use crate::acp::registry::{self, AgentDistribution};
use crate::models::agent::AgentType;

use super::npm_install::{
    contract_error, ManagedNpmInstall, ManagedNpmInstallError, ManagedNpmPackage,
};
use super::uvx_install::ManagedUvxInstall;

pub(crate) fn fallback_npm_agent_install(
    agent_type: AgentType,
    requested_version: &str,
) -> Result<ManagedNpmInstall, ManagedNpmInstallError> {
    let AgentDistribution::Npx { package, .. } = registry::get_agent_meta(agent_type).distribution
    else {
        return Err(ManagedNpmInstallError::Rejected(contract_error(
            "version center Agent is not npm-based",
        )));
    };
    let effective_version = if agent_type == AgentType::DeepSeek {
        crate::acp::deepseek_config::fallback_tool_version(agent_type).ok_or_else(|| {
            ManagedNpmInstallError::Rejected(contract_error(
                "DeepSeek Harness trusted version is unavailable",
            ))
        })?
    } else {
        requested_version
    };
    let package_name = package_name(package);
    let registry = "https://registry.npmjs.org".to_string();
    let mut packages = vec![ManagedNpmPackage {
        component_key: registry::registry_id_for(agent_type).to_string(),
        package_name: package_name.to_string(),
        install_spec: format!("{package_name}@{effective_version}"),
        package_version: effective_version.to_string(),
        registry: registry.clone(),
        integrity: String::new(),
        source_key: "official-npm-registry".to_string(),
    }];
    if agent_type == AgentType::Pi {
        const PI_CODING_AGENT_VERSION: &str = "0.84.1";
        packages.push(ManagedNpmPackage {
            component_key: "pi-coding-agent".to_string(),
            package_name: "@earendil-works/pi-coding-agent".to_string(),
            install_spec: format!("@earendil-works/pi-coding-agent@{PI_CODING_AGENT_VERSION}"),
            package_version: PI_CODING_AGENT_VERSION.to_string(),
            registry,
            integrity: String::new(),
            source_key: "official-npm-registry".to_string(),
        });
    }
    Ok(ManagedNpmInstall {
        version: effective_version.to_string(),
        revision: 0,
        effective_policy: "manual".to_string(),
        packages,
        bundle_offer: None,
    })
}

pub(crate) fn fallback_uvx_agent_install(
    agent_type: AgentType,
    requested_version: &str,
) -> Result<ManagedUvxInstall, ManagedNpmInstallError> {
    let AgentDistribution::Uvx { package, .. } = registry::get_agent_meta(agent_type).distribution
    else {
        return Err(ManagedNpmInstallError::Rejected(contract_error(
            "version center Agent is not uvx-based",
        )));
    };
    let base = package.split("==").next().unwrap_or(package).trim();
    Ok(ManagedUvxInstall {
        version: requested_version.to_string(),
        revision: 0,
        effective_policy: "manual".to_string(),
        package_spec: format!("{base}=={requested_version}"),
        index_url: "https://pypi.org/simple".to_string(),
        source_key: "official-pypi".to_string(),
        bundle_offer: None,
    })
}

fn package_name(spec: &str) -> &str {
    let spec = spec.trim();
    if spec.starts_with('@') {
        spec[1..]
            .find('@')
            .map(|index| &spec[..index + 1])
            .unwrap_or(spec)
    } else {
        spec.split('@').next().unwrap_or(spec)
    }
}
