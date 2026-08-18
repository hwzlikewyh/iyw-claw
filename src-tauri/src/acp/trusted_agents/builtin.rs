//! Definitions that Codeg v0.26.1 implements as dedicated Agent SDK entries.

use super::types::{trusted_agent, TrustedAgentDefinition, ACP_FULL_SESSION, ACP_WITH_MCP};

#[rustfmt::skip]
pub static BUILTIN_TRUSTED_AGENTS: [TrustedAgentDefinition; 2] = [
    trusted_agent!(
        "cursor", "Cursor", ManagedBinary, "cursor-agent",
        "dist-package/cursor-agent.cmd", &["acp"], &[], &[], Bundled, None,
        "2026.08.11-e8db854", ACP_WITH_MCP
    ),
    trusted_agent!(
        // 0.3.0 no longer resolves after the upstream RC dependency set moved.
        "deepseek-acp", "DeepSeek Harness", Npx, "deepseek-acp@0.5.0",
        "deepseek-acp", &[], &[], &[
            "DEEPSEEK_API_KEY",
            "DEEPSEEK_BASE_URL",
            "DEEPSEEK_ACP_PROVIDER",
            "DEEPSEEK_ACP_MODEL",
        ], Node, Some("22.0.0"),
        "0.5.0", ACP_FULL_SESSION
    ),
];
