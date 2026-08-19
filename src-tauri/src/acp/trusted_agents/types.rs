//! Compile-time trusted Agent launch definitions.
//!
//! These records deliberately carry only client-reviewed constants. Fusion may
//! select a signed version and artifact for a known `registry_id`, but it must
//! never supply a command, argument, environment value, or package recipe.

/// The only delivery channels accepted by the trusted directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryKind {
    Npx,
    Uvx,
    ManagedBinary,
}

/// The runtime whose version is checked before an Agent can launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    Node,
    Uv,
    Python,
    Bundled,
}

/// Fixed process launch data. `allowed_env_names` excludes fixed values and
/// only permits local, user-owned configuration to populate those names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchSpec {
    pub entrypoint: &'static str,
    pub fixed_args: &'static [&'static str],
    pub fixed_env: &'static [(&'static str, &'static str)],
    pub allowed_env_names: &'static [&'static str],
}

/// The floor an installed runtime and Agent binary/package must satisfy.
///
/// `minimum_runtime_version: None` means the upstream manifest publishes no
/// runtime floor. It does not bypass the separate runtime-verification gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionFloor {
    pub runtime: RuntimeKind,
    pub minimum_runtime_version: Option<&'static str>,
    pub minimum_tool_version: &'static str,
}

/// Capability statements backed by protocol evidence, not optimistic defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolCapabilities {
    pub acp: bool,
    pub mcp: bool,
    pub resume: bool,
    pub load: bool,
}

/// One client-reviewed, immutable Agent definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedAgentDefinition {
    pub registry_id: &'static str,
    pub display_name: &'static str,
    pub delivery: DeliveryKind,
    /// Immutable npm/PyPI package spec or managed binary identity.
    pub package_or_binary: &'static str,
    pub launch: LaunchSpec,
    pub version_floor: VersionFloor,
    pub capabilities: ProtocolCapabilities,
}

pub const ACP_ONLY: ProtocolCapabilities = ProtocolCapabilities {
    acp: true,
    mcp: false,
    resume: false,
    load: false,
};

pub const ACP_WITH_MCP: ProtocolCapabilities = ProtocolCapabilities {
    acp: true,
    mcp: true,
    resume: false,
    load: false,
};

pub const ACP_FULL_SESSION: ProtocolCapabilities = ProtocolCapabilities {
    acp: true,
    mcp: true,
    resume: true,
    load: true,
};

macro_rules! trusted_agent {
    (
        $id:literal, $name:literal, $delivery:ident, $identity:literal,
        $entrypoint:literal, $args:expr, $fixed_env:expr, $allowed_env:expr,
        $runtime:ident, $runtime_minimum:expr, $tool_minimum:literal, $capabilities:expr
    ) => {
        $crate::acp::trusted_agents::TrustedAgentDefinition {
            registry_id: $id,
            display_name: $name,
            delivery: $crate::acp::trusted_agents::DeliveryKind::$delivery,
            package_or_binary: $identity,
            launch: $crate::acp::trusted_agents::LaunchSpec {
                entrypoint: $entrypoint,
                fixed_args: $args,
                fixed_env: $fixed_env,
                allowed_env_names: $allowed_env,
            },
            version_floor: $crate::acp::trusted_agents::VersionFloor {
                runtime: $crate::acp::trusted_agents::RuntimeKind::$runtime,
                minimum_runtime_version: $runtime_minimum,
                minimum_tool_version: $tool_minimum,
            },
            capabilities: $capabilities,
        }
    };
}

pub(crate) use trusted_agent;
