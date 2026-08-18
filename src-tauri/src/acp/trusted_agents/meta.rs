use crate::acp::registry::{AcpAgentMeta, AgentDistribution};
use crate::models::agent::AgentType;

use super::{DeliveryKind, RuntimeKind, TrustedAgentDefinition};

pub(crate) fn meta_for(
    agent_type: AgentType,
    definition: &'static TrustedAgentDefinition,
) -> AcpAgentMeta {
    let distribution = match definition.delivery {
        DeliveryKind::Npx => AgentDistribution::Npx {
            version: definition.version_floor.minimum_tool_version,
            package: definition.package_or_binary,
            cmd: definition.launch.entrypoint,
            args: definition.launch.fixed_args,
            env: definition.launch.fixed_env,
            node_required: runtime_floor(definition, RuntimeKind::Node),
        },
        DeliveryKind::Uvx => AgentDistribution::Uvx {
            version: definition.version_floor.minimum_tool_version,
            package: definition.package_or_binary,
            cmd: definition.launch.entrypoint,
            args: definition.launch.fixed_args,
            env: definition.launch.fixed_env,
            uv_required: runtime_floor(definition, RuntimeKind::Uv),
            python: runtime_floor(definition, RuntimeKind::Python),
            system_cmd: None,
        },
        DeliveryKind::ManagedBinary => AgentDistribution::Binary {
            version: definition.version_floor.minimum_tool_version,
            cmd: definition.launch.entrypoint,
            args: definition.launch.fixed_args,
            env: definition.launch.fixed_env,
            // Fusion owns artifact selection; transport wiring stays outside
            // the compile-time trusted definition.
            platforms: &[],
        },
    };
    AcpAgentMeta {
        agent_type,
        // The trusted definition is only the compile-time capability claim;
        // runtime policy and the Agent's initialize response still gate MCP
        // forwarding before any server or authority is issued.
        supports_mcp: definition.capabilities.mcp,
        name: definition.display_name,
        description: "Client-reviewed ACP Agent definition",
        distribution,
    }
}

pub(crate) fn unavailable_meta(agent_type: AgentType, registry_id: &'static str) -> AcpAgentMeta {
    AcpAgentMeta {
        agent_type,
        supports_mcp: false,
        name: registry_id,
        description: "Unknown Agent definition; launch is disabled",
        distribution: AgentDistribution::Binary {
            version: "0.0.0",
            cmd: "",
            args: &[],
            env: &[],
            platforms: &[],
        },
    }
}

fn runtime_floor(
    definition: &'static TrustedAgentDefinition,
    runtime: RuntimeKind,
) -> Option<&'static str> {
    (definition.version_floor.runtime == runtime)
        .then_some(definition.version_floor.minimum_runtime_version)
        .flatten()
}
