use crate::acp::trusted_agents;
use crate::models::agent::AgentType;
use serde::{Deserialize, Serialize};

mod builtin_meta;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentConcurrencyEnforcement {
    NativeAndHost,
    Host,
}

pub fn subagent_concurrency_enforcement(agent_type: AgentType) -> SubagentConcurrencyEnforcement {
    match agent_type {
        AgentType::Codex | AgentType::ClaudeCode => SubagentConcurrencyEnforcement::NativeAndHost,
        _ => SubagentConcurrencyEnforcement::Host,
    }
}

const TRUSTED_MANAGED_BINARY_PLATFORM: &str = "windows-x86_64";

#[derive(Debug, Clone)]
pub enum AgentDistribution {
    Npx {
        version: &'static str,
        package: &'static str,
        /// The command name provided by this npx package (e.g. "gemini", "openclaw").
        cmd: &'static str,
        args: &'static [&'static str],
        env: &'static [(&'static str, &'static str)],
        /// Minimum Node.js version required, e.g. "22.12.0". None means no specific requirement.
        node_required: Option<&'static str>,
    },
    Binary {
        version: &'static str,
        cmd: &'static str,
        args: &'static [&'static str],
        env: &'static [(&'static str, &'static str)],
        platforms: &'static [PlatformBinary],
    },
    /// Python agents launched through `uvx` (the `uv` tool runner), which
    /// fetches + caches the pinned package on first use — analogous to npx.
    /// Used for ACP agents distributed as Python packages (e.g. Hermes).
    Uvx {
        version: &'static str,
        /// The `uvx --from` package spec, e.g. "hermes-agent[acp,mcp]==0.18.0".
        package: &'static str,
        /// The console-script entry point to run, e.g. "hermes-acp".
        cmd: &'static str,
        args: &'static [&'static str],
        env: &'static [(&'static str, &'static str)],
        /// Minimum `uv` version required, e.g. "0.5.0". None means no specific requirement.
        uv_required: Option<&'static str>,
        /// Interpreter to pin via `uvx --python <ver>`, e.g. `Some("3.13")`.
        /// `None` lets uvx pick its default interpreter. Set this when the
        /// package (or a transitive dep) does not support the machine's default
        /// Python — uv auto-downloads a managed build of the pinned version.
        python: Option<&'static str>,
        /// Fallback command resolvable on PATH when `uvx` is unavailable, e.g.
        /// `Some(("hermes", &["acp"]))` — lets users who installed the agent via
        /// its official installer launch it without `uv`.
        system_cmd: Option<(&'static str, &'static [&'static str])>,
    },
}

#[derive(Debug, Clone)]
pub struct PlatformBinary {
    pub platform: &'static str,
    pub url: &'static str,
}

#[derive(Debug, Clone)]
pub struct AcpAgentMeta {
    pub agent_type: AgentType,
    /// 是否经 ACP 线缆（session/new 的 `mcpServers` 字段）向该 agent 转发 MCP
    /// 服务器——既包括用户配置的服务器，也包括内置 iyw-claw-mcp 伴生进程。
    /// OpenClaw 拒绝 `mcpServers` 中的任何服务器条目（会使 session/new 失败），
    /// 故置 false。注意空列表 `[]` 仍会按 ACP schema 序列化、OpenClaw 可接受——
    /// 闸门只是保证该列表对 OpenClaw 恒为空（不含任何条目）。
    pub supports_mcp: bool,
    pub name: &'static str,
    pub description: &'static str,
    pub distribution: AgentDistribution,
}

impl AcpAgentMeta {
    pub fn registry_version(&self) -> Option<&'static str> {
        match &self.distribution {
            AgentDistribution::Npx { version, .. }
            | AgentDistribution::Binary { version, .. }
            | AgentDistribution::Uvx { version, .. } => Some(*version),
        }
    }
}

fn platform_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("darwin-aarch64"),
        ("macos", "x86_64") => Some("darwin-x86_64"),
        ("linux", "aarch64") => Some("linux-aarch64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("windows", "aarch64") => Some("windows-aarch64"),
        ("windows", "x86") => Some("windows-i686"),
        ("windows", "x86_64") => Some("windows-x86_64"),
        _ => None,
    }
}

pub fn current_platform() -> &'static str {
    platform_for(std::env::consts::OS, std::env::consts::ARCH).unwrap_or_else(|| {
        panic!(
            "unsupported platform: {}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })
}

pub fn binary_platform_supported(agent_type: AgentType, platforms: &[PlatformBinary]) -> bool {
    platforms
        .iter()
        .any(|platform| platform.platform == current_platform())
        || (platforms.is_empty()
            && current_platform() == TRUSTED_MANAGED_BINARY_PLATFORM
            && trusted_agents::definition_for_agent(agent_type).is_some_and(|definition| {
                definition.delivery == trusted_agents::DeliveryKind::ManagedBinary
            }))
}

pub fn all_acp_agents() -> Vec<AgentType> {
    builtin_agent_types()
}

/// All built-in identities, including runtimes implemented by Task 04.
pub fn builtin_agent_types() -> Vec<AgentType> {
    crate::models::agent::BUILTIN_AGENT_TYPES.to_vec()
}

/// The 13 built-ins plus this release's 28 reviewed Registry identities.
pub fn all_identity_agents() -> Vec<AgentType> {
    let mut agents = builtin_agent_types();
    agents.extend(
        trusted_agents::REGISTRY_TRUSTED_AGENTS
            .iter()
            .filter_map(|definition| AgentType::custom(definition.registry_id)),
    );
    agents
}

pub fn registry_id_for(agent_type: AgentType) -> &'static str {
    match agent_type {
        AgentType::ClaudeCode => "claude-acp",
        AgentType::Codex => "codex-acp",
        AgentType::Gemini => "gemini",
        AgentType::OpenClaw => "openclaw-acp",
        AgentType::OpenCode => "opencode",
        AgentType::Cline => "cline",
        AgentType::Hermes => "hermes",
        AgentType::CodeBuddy => "codebuddy-code",
        AgentType::KimiCode => "kimi-code",
        AgentType::Pi => "pi-acp",
        AgentType::Grok => "grok-build",
        AgentType::Cursor => "cursor",
        AgentType::DeepSeek => "deepseek-acp",
        AgentType::Custom(registry_id) => registry_id,
    }
}

pub fn from_registry_id(id: &str) -> Option<AgentType> {
    match id {
        "claude-acp" => Some(AgentType::ClaudeCode),
        "codex-acp" => Some(AgentType::Codex),
        "gemini" => Some(AgentType::Gemini),
        "openclaw-acp" => Some(AgentType::OpenClaw),
        "opencode" => Some(AgentType::OpenCode),
        "cline" => Some(AgentType::Cline),
        "hermes" => Some(AgentType::Hermes),
        "codebuddy-code" => Some(AgentType::CodeBuddy),
        "kimi-code" => Some(AgentType::KimiCode),
        "pi-acp" => Some(AgentType::Pi),
        "grok-build" => Some(AgentType::Grok),
        "cursor" => Some(AgentType::Cursor),
        "deepseek-acp" => Some(AgentType::DeepSeek),
        other => trusted_agents::definition_for(other).and_then(|_| AgentType::custom(other)),
    }
}

pub fn is_executable_identity(agent_type: AgentType) -> bool {
    !agent_type.is_custom() || trusted_agents::definition_for_agent(agent_type).is_some()
}

pub fn try_get_agent_meta(agent_type: AgentType) -> Option<AcpAgentMeta> {
    is_executable_identity(agent_type).then(|| get_agent_meta(agent_type))
}

pub fn get_agent_meta(agent_type: AgentType) -> AcpAgentMeta {
    if let Some(definition) = trusted_agents::definition_for_agent(agent_type) {
        return trusted_agents::meta_for(agent_type, definition);
    }
    if let AgentType::Custom(registry_id) = agent_type {
        return trusted_agents::unavailable_meta(agent_type, registry_id);
    }
    debug_assert_eq!(
        from_registry_id(registry_id_for(agent_type)),
        Some(agent_type)
    );
    builtin_meta::get(agent_type)
}
