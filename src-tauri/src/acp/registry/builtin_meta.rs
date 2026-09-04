use crate::models::agent::AgentType;

use super::{AcpAgentMeta, AgentDistribution, PlatformBinary};

pub(super) fn get(agent_type: AgentType) -> AcpAgentMeta {
    match agent_type {
        AgentType::ClaudeCode => AcpAgentMeta {
            agent_type,
            supports_mcp: true,
            name: "远山",
            description: "ACP wrapper for Anthropic's Claude",
            distribution: AgentDistribution::Npx {
                version: "0.73.0",
                package: "@agentclientprotocol/claude-agent-acp@0.73.0",
                cmd: "claude-agent-acp",
                args: &[],
                env: &[],
                node_required: Some("22.0.0"),
            },
        },
        AgentType::Codex => AcpAgentMeta {
            agent_type,
            supports_mcp: true,
            name: "星河",
            description: "ACP adapter for OpenAI's coding assistant",
            // codex-acp moved from zed-industries (Rust binary) to the
            // agentclientprotocol org (TypeScript rewrite, npx-distributed).
            // Since 1.0.1 it resolves the resumed `model_provider` from
            // `~/.codex/config.toml` (#224), so iyw-claw no longer injects
            // `MODEL_PROVIDER`. Since 1.1.0 (#263), `/goal` transitions arrive
            // as structured `session_info_update` values (`_meta.codex.goal`)
            // rather than live agent text; see `crate::acp::codex_goal`.
            distribution: AgentDistribution::Npx {
                version: "1.8.0",
                package: "@agentclientprotocol/codex-acp@1.8.0",
                cmd: "codex-acp",
                args: &[],
                env: &[],
                node_required: Some("20.0.0"),
            },
        },
        AgentType::Gemini => AcpAgentMeta {
            agent_type,
            supports_mcp: true,
            name: "流光",
            description: "Google's official CLI for Gemini",
            distribution: AgentDistribution::Npx {
                version: "0.58.0",
                package: "@google/gemini-cli@0.58.0",
                cmd: "gemini",
                args: &["--acp", "--skip-trust"],
                env: &[],
                node_required: Some("20.0.0"),
            },
        },
        AgentType::OpenClaw => AcpAgentMeta {
            agent_type,
            // OpenClaw rejects non-empty `mcpServers`, so no MCP entries may
            // be forwarded to it, including the iyw-claw companion.
            supports_mcp: false,
            name: "开放之爪",
            description: "OpenClaw is a personal AI assistant you run on your own devices.",
            distribution: AgentDistribution::Npx {
                version: "2026.8.2",
                package: "openclaw@2026.8.2",
                cmd: "openclaw",
                args: &["acp"],
                env: &[],
                node_required: Some("22.22.3"),
            },
        },
        AgentType::Cline => AcpAgentMeta {
            agent_type,
            supports_mcp: true,
            name: "逐风",
            description: "Autonomous coding agent CLI",
            distribution: AgentDistribution::Npx {
                version: "3.0.61",
                package: "cline@3.0.61",
                cmd: "cline",
                args: &["--acp"],
                env: &[],
                node_required: None,
            },
        },
        AgentType::OpenCode => AcpAgentMeta {
            agent_type,
            supports_mcp: true,
            name: "云舟",
            description: "The open source coding agent",
            distribution: AgentDistribution::Binary {
                version: "1.18.27",
                cmd: "opencode",
                args: &["acp"],
                env: &[],
                platforms: &[
                    PlatformBinary {
                        platform: "darwin-aarch64",
                        url: "https://github.com/anomalyco/opencode/releases/download/v1.18.27/opencode-darwin-arm64.zip",
                    },
                    PlatformBinary {
                        platform: "darwin-x86_64",
                        url: "https://github.com/anomalyco/opencode/releases/download/v1.18.27/opencode-darwin-x64.zip",
                    },
                    PlatformBinary {
                        platform: "linux-aarch64",
                        url: "https://github.com/anomalyco/opencode/releases/download/v1.18.27/opencode-linux-arm64.tar.gz",
                    },
                    PlatformBinary {
                        platform: "linux-x86_64",
                        url: "https://github.com/anomalyco/opencode/releases/download/v1.18.27/opencode-linux-x64.tar.gz",
                    },
                    PlatformBinary {
                        platform: "windows-aarch64",
                        url: "https://github.com/anomalyco/opencode/releases/download/v1.18.27/opencode-windows-arm64.zip",
                    },
                    PlatformBinary {
                        platform: "windows-x86_64",
                        url: "https://github.com/anomalyco/opencode/releases/download/v1.18.27/opencode-windows-x64.zip",
                    },
                ],
            },
        },
        AgentType::Hermes => AcpAgentMeta {
            agent_type,
            supports_mcp: true,
            name: "赫尔墨斯",
            description: "Nous Research's self-improving agent (ACP via uvx)",
            distribution: AgentDistribution::Uvx {
                version: "0.19.0",
                package: "hermes-agent[acp,mcp]==0.19.0",
                cmd: "hermes-acp",
                args: &[],
                env: &[],
                uv_required: Some("0.5.0"),
                // Hermes supports Python 3.11 through 3.13. Pinning 3.13 avoids
                // uv selecting Python 3.14, for which pywinpty has no wheel.
                python: Some("3.13"),
                system_cmd: Some(("hermes", &["acp"])),
            },
        },
        AgentType::CodeBuddy => AcpAgentMeta {
            agent_type,
            supports_mcp: true,
            name: "青岚",
            description: "Tencent Cloud's official AI coding assistant (ACP)",
            distribution: AgentDistribution::Npx {
                version: "2.143.1",
                package: "@tencent-ai/codebuddy-code@2.143.1",
                cmd: "codebuddy",
                args: &["--acp"],
                env: &[],
                node_required: Some("22.0.0"),
            },
        },
        AgentType::KimiCode => AcpAgentMeta {
            agent_type,
            supports_mcp: true,
            name: "月白",
            description: "Moonshot AI's official CLI coding assistant (ACP)",
            distribution: AgentDistribution::Npx {
                version: "0.40.1",
                package: "@moonshot-ai/kimi-code@0.40.1",
                cmd: "kimi",
                args: &["acp"],
                env: &[],
                node_required: Some("22.19.0"),
            },
        },
        AgentType::Pi => AcpAgentMeta {
            agent_type,
            // pi-acp accepts ACP-wire `mcpServers` but drops them. Actual wire
            // forwarding remains disabled in connection.rs.
            supports_mcp: true,
            name: "墨川",
            description: "Self-extensible coding agent (ACP via pi-acp)",
            distribution: AgentDistribution::Npx {
                version: "0.0.33",
                package: "pi-acp@0.0.33",
                cmd: "pi-acp",
                args: &[],
                env: &[("PI_ACP_ENABLE_EMBEDDED_CONTEXT", "true")],
                node_required: Some("22.19.0"),
            },
        },
        AgentType::Grok => AcpAgentMeta {
            agent_type,
            supports_mcp: true,
            name: "知微",
            description: "xAI's official coding agent and CLI (ACP via grok agent stdio)",
            distribution: AgentDistribution::Npx {
                version: "1.0.18",
                package: "@xai-official/grok@1.0.18",
                cmd: "grok",
                args: &["agent", "stdio"],
                env: &[],
                node_required: Some("20.0.0"),
            },
        },
        AgentType::Cursor | AgentType::DeepSeek | AgentType::Custom(_) => {
            unreachable!("trusted identities resolve before the built-in registry match")
        }
    }
}
