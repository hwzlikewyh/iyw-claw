//! Static, fail-closed launch definitions for Agents added after the original
//! iyw-claw built-in registry.
//!
//! Fusion may select a known platform id and version, but only definitions in
//! this module may contribute launch metadata.

use std::collections::BTreeMap;

mod builtin;
mod fingerprint;
mod meta;
mod projection;
mod registry;
mod types;

use crate::models::agent::AgentType;

use fingerprint::{fingerprint_definition, fingerprint_legacy_meta};

pub use builtin::BUILTIN_TRUSTED_AGENTS;
pub(crate) use meta::{meta_for, unavailable_meta};
pub use projection::{project_catalog, TrustedAgentProjection};
pub use registry::REGISTRY_TRUSTED_AGENTS;
pub use types::{
    DeliveryKind, LaunchSpec, ProtocolCapabilities, RuntimeKind, TrustedAgentDefinition,
    VersionFloor,
};

pub const TRUSTED_AGENT_COUNT: usize = BUILTIN_TRUSTED_AGENTS.len() + REGISTRY_TRUSTED_AGENTS.len();

pub(crate) struct RuntimeIdentity {
    pub(crate) definition_fingerprint: String,
    pub(crate) runtime_version: String,
}

pub(crate) fn runtime_identity(agent_type: AgentType) -> Option<RuntimeIdentity> {
    if let Some(definition) = definition_for_agent(agent_type) {
        return Some(RuntimeIdentity {
            definition_fingerprint: fingerprint_definition(definition),
            runtime_version: definition.version_floor.minimum_tool_version.to_string(),
        });
    }
    if agent_type.is_custom() {
        return None;
    }
    let meta = crate::acp::registry::try_get_agent_meta(agent_type)?;
    let runtime_version = meta.registry_version()?.to_string();
    Some(RuntimeIdentity {
        definition_fingerprint: fingerprint_legacy_meta(&meta),
        runtime_version,
    })
}

pub fn definition_for(registry_id: &str) -> Option<&'static TrustedAgentDefinition> {
    BUILTIN_TRUSTED_AGENTS
        .iter()
        .chain(REGISTRY_TRUSTED_AGENTS.iter())
        .find(|definition| definition.registry_id == registry_id)
}

pub fn all_definitions() -> impl Iterator<Item = &'static TrustedAgentDefinition> {
    BUILTIN_TRUSTED_AGENTS
        .iter()
        .chain(REGISTRY_TRUSTED_AGENTS.iter())
}

pub fn definition_for_agent(agent_type: AgentType) -> Option<&'static TrustedAgentDefinition> {
    definition_for(crate::acp::registry::registry_id_for(agent_type))
}

/// Keep user/provider configuration limited to the names reviewed for this
/// trusted definition. Runtime-owned values are explicitly enumerated so the
/// same gate can safely run again at the manager's final launch boundary.
pub(crate) fn restrict_configured_runtime_env(
    agent_type: AgentType,
    environment: &mut BTreeMap<String, String>,
) {
    let Some(definition) = definition_for_agent(agent_type) else {
        return;
    };
    let allowed = definition.launch.allowed_env_names;
    let before = environment.len();
    environment
        .retain(|key, _| is_host_owned_runtime_env(key) || allowed.iter().any(|name| *name == key));
    let rejected = before.saturating_sub(environment.len());
    if rejected > 0 {
        tracing::debug!(
            agent = definition.registry_id,
            rejected,
            "[ACP] restricted trusted Agent runtime configuration environment"
        );
    }
}

fn is_host_owned_runtime_env(key: &str) -> bool {
    key.eq_ignore_ascii_case("PATH")
        || key == "IYW_CLAW_MANAGED_AGENT_VERSION"
        || key == "IYW_CLAW_TOOL_BIN"
        || key == "IYW_CLAW_TOOL_SOCKET"
        || key == "IYW_CLAW_AGENT_TYPE"
        || key == crate::wecom_ai::CONFIG_DIR_ENV
        || key == crate::wecom_ai::MANAGED_COMMAND_ENV
        || key == "OPENCLAW_RESET_SESSION"
        || key == "PI_ACP_PI_COMMAND"
        || key.eq_ignore_ascii_case("DSH_HOME")
        || key.eq_ignore_ascii_case("DEEPSEEK_ACP_SESSIONS_ROOT")
        || key.starts_with("GIT_CONFIG_")
}

pub(crate) fn minimum_node_version(agent_type: AgentType) -> Option<&'static str> {
    let definition = definition_for_agent(agent_type)?;
    (definition.version_floor.runtime == RuntimeKind::Node)
        .then_some(definition.version_floor.minimum_runtime_version)
        .flatten()
}
