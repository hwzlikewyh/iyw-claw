use sha2::{Digest, Sha256};

use crate::acp::registry::{AcpAgentMeta, AgentDistribution};

use super::{DeliveryKind, RuntimeKind, TrustedAgentDefinition};

const DEFINITION_FINGERPRINT_DOMAIN: &[u8] = b"iyw-claw:trusted-agent-definition:v1\0";

pub(super) fn fingerprint_definition(definition: &TrustedAgentDefinition) -> String {
    let mut hasher = DefinitionHasher::new();
    hasher.text(definition.registry_id);
    hasher.text(definition.display_name);
    hasher.text(delivery_kind(definition.delivery));
    hasher.text(definition.package_or_binary);
    hasher.text(definition.launch.entrypoint);
    hasher.texts(definition.launch.fixed_args);
    hasher.pairs(definition.launch.fixed_env);
    hasher.texts(definition.launch.allowed_env_names);
    hasher.text(runtime_kind(definition.version_floor.runtime));
    hasher.optional_text(definition.version_floor.minimum_runtime_version);
    hasher.text(definition.version_floor.minimum_tool_version);
    hasher.flag(definition.capabilities.acp);
    hasher.flag(definition.capabilities.mcp);
    hasher.flag(definition.capabilities.resume);
    hasher.flag(definition.capabilities.load);
    hasher.finish()
}

pub(super) fn fingerprint_legacy_meta(meta: &AcpAgentMeta) -> String {
    let mut hasher = DefinitionHasher::new();
    hasher.text(crate::acp::registry::registry_id_for(meta.agent_type));
    hasher.text(meta.name);
    hasher.text(meta.description);
    hasher.flag(meta.supports_mcp);
    hash_distribution(&mut hasher, &meta.distribution);
    hasher.finish()
}

fn hash_distribution(hasher: &mut DefinitionHasher, distribution: &AgentDistribution) {
    match distribution {
        AgentDistribution::Npx { .. } => hash_npx_distribution(hasher, distribution),
        AgentDistribution::Binary { .. } => hash_binary_distribution(hasher, distribution),
        AgentDistribution::Uvx { .. } => hash_uvx_distribution(hasher, distribution),
    }
}

fn hash_npx_distribution(hasher: &mut DefinitionHasher, distribution: &AgentDistribution) {
    let AgentDistribution::Npx {
        version,
        package,
        cmd,
        args,
        env,
        node_required,
    } = distribution
    else {
        return;
    };
    hasher.text("npx");
    hasher.text(version);
    hasher.text(package);
    hasher.text(cmd);
    hasher.texts(args);
    hasher.pairs(env);
    hasher.optional_text(*node_required);
}

fn hash_binary_distribution(hasher: &mut DefinitionHasher, distribution: &AgentDistribution) {
    let AgentDistribution::Binary {
        version,
        cmd,
        args,
        env,
        platforms,
    } = distribution
    else {
        return;
    };
    hasher.text("binary");
    hasher.text(version);
    hasher.text(cmd);
    hasher.texts(args);
    hasher.pairs(env);
    hasher.count(platforms.len());
    for platform in *platforms {
        hasher.text(platform.platform);
        hasher.text(platform.url);
    }
}

fn hash_uvx_distribution(hasher: &mut DefinitionHasher, distribution: &AgentDistribution) {
    let AgentDistribution::Uvx {
        version,
        package,
        cmd,
        args,
        env,
        uv_required,
        python,
        system_cmd,
    } = distribution
    else {
        return;
    };
    hasher.text("uvx");
    hasher.text(version);
    hasher.text(package);
    hasher.text(cmd);
    hasher.texts(args);
    hasher.pairs(env);
    hasher.optional_text(*uv_required);
    hasher.optional_text(*python);
    hash_system_command(hasher, *system_cmd);
}

fn hash_system_command(
    hasher: &mut DefinitionHasher,
    system_cmd: Option<(&'static str, &'static [&'static str])>,
) {
    match system_cmd {
        Some((command, args)) => {
            hasher.flag(true);
            hasher.text(command);
            hasher.texts(args);
        }
        None => hasher.flag(false),
    }
}

fn delivery_kind(delivery: DeliveryKind) -> &'static str {
    match delivery {
        DeliveryKind::Npx => "npx",
        DeliveryKind::Uvx => "uvx",
        DeliveryKind::ManagedBinary => "managed-binary",
    }
}

fn runtime_kind(runtime: RuntimeKind) -> &'static str {
    match runtime {
        RuntimeKind::Node => "node",
        RuntimeKind::Uv => "uv",
        RuntimeKind::Python => "python",
        RuntimeKind::Bundled => "bundled",
    }
}

struct DefinitionHasher(Sha256);

impl DefinitionHasher {
    fn new() -> Self {
        let mut hasher = Sha256::new();
        hasher.update(DEFINITION_FINGERPRINT_DOMAIN);
        Self(hasher)
    }

    fn count(&mut self, value: usize) {
        self.0.update((value as u64).to_le_bytes());
    }

    fn flag(&mut self, value: bool) {
        self.0.update([u8::from(value)]);
    }

    fn text(&mut self, value: &str) {
        self.count(value.len());
        self.0.update(value.as_bytes());
    }

    fn optional_text(&mut self, value: Option<&str>) {
        self.flag(value.is_some());
        if let Some(value) = value {
            self.text(value);
        }
    }

    fn texts(&mut self, values: &[&str]) {
        self.count(values.len());
        for value in values {
            self.text(value);
        }
    }

    fn pairs(&mut self, values: &[(&str, &str)]) {
        self.count(values.len());
        for (key, value) in values {
            self.text(key);
            self.text(value);
        }
    }

    fn finish(self) -> String {
        format!("{:x}", self.0.finalize())
    }
}
