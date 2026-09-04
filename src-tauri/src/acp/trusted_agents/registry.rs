//! Frozen ACP Registry definitions accepted by this client release.
//!
//! Binary entrypoints are the Registry's `windows-x86_64` archive paths. The
//! Fusion installer owns URL, checksum, signature, and archive validation.

use super::types::{trusted_agent, TrustedAgentDefinition, ACP_ONLY};

#[rustfmt::skip]
pub static REGISTRY_TRUSTED_AGENTS: [TrustedAgentDefinition; 28] = [
    trusted_agent!(
        "agoragentic-acp", "Agoragentic", Npx, "agoragentic-mcp@1.3.6",
        "agoragentic-mcp", &["--acp"], &[], &[], Node, Some("18.0.0"), "1.3.6", ACP_ONLY
    ),
    trusted_agent!(
        "amp-acp", "Amp", ManagedBinary, "amp-acp", "amp-acp.exe", &[], &[], &[], Bundled,
        None, "0.9.0", ACP_ONLY
    ),
    trusted_agent!(
        "auggie", "Auggie CLI", Npx, "@augmentcode/auggie@0.36.0", "auggie", &["--acp"],
        &[("AUGMENT_DISABLE_AUTO_UPDATE", "1")], &[], Node, Some("20.0.0"), "0.36.0", ACP_ONLY
    ),
    trusted_agent!(
        "autohand", "Autohand Code", Npx, "@autohandai/autohand-acp@0.2.1", "autohand-acp",
        &[], &[], &[], Node, Some("18.17.0"), "0.2.1", ACP_ONLY
    ),
    trusted_agent!(
        "cortex-code", "Cortex Code", ManagedBinary, "cortex", "./coco-1.0.73+180523.e6179a031de9-windows-amd64/cortex.exe",
        &["acp", "serve"], &[], &[], Bundled, None, "1.0.73", ACP_ONLY
    ),
    trusted_agent!(
        "corust-agent", "Corust Agent", ManagedBinary, "corust-agent-acp", "./corust-agent-acp.exe",
        &[], &[], &[], Bundled, None, "0.6.0", ACP_ONLY
    ),
    trusted_agent!(
        "crow-cli", "crow-cli", ManagedBinary, "crow-cli", "./crow-cli.exe", &["acp"], &[], &[],
        Bundled, None, "0.1.24", ACP_ONLY
    ),
    trusted_agent!(
        "deepagents", "DeepAgents", Npx, "deepagents-acp@0.1.28", "deepagents-acp", &[], &[], &[],
        Node, None, "0.1.28", ACP_ONLY
    ),
    trusted_agent!(
        "devin", "Devin", ManagedBinary, "devin", "./bin\\devin.exe", &["acp"], &[], &[], Bundled,
        None, "3000.6.14", ACP_ONLY
    ),
    trusted_agent!(
        "dimcode", "DimCode", Npx, "dimcode@0.3.28", "dimcode", &["acp"], &[], &[], Node, None,
        "0.3.28", ACP_ONLY
    ),
    trusted_agent!(
        "dirac", "Dirac", Npx, "dirac-cli@0.5.4", "dirac", &["--acp"], &[], &[], Node,
        Some("22.13.0"), "0.5.4", ACP_ONLY
    ),
    trusted_agent!(
        "factory-droid", "Factory Droid", Npx, "droid@0.211.0", "droid",
        &["exec", "--output-format", "acp-daemon"],
        &[("DROID_DISABLE_AUTO_UPDATE", "true"), ("FACTORY_DROID_AUTO_UPDATE_ENABLED", "false")],
        &[], Node, Some("20.0.0"), "0.211.0", ACP_ONLY
    ),
    trusted_agent!(
        "fast-agent", "fast-agent", Uvx, "fast-agent-acp==0.10.16", "fast-agent-acp", &["-x"],
        &[("FAST_AGENT_MODEL", "codexplan")], &[], Python, Some("3.12"), "0.10.16", ACP_ONLY
    ),
    trusted_agent!(
        "github-copilot-cli", "GitHub Copilot", Npx, "@github/copilot@1.0.82", "copilot", &["--acp"],
        &[], &[], Node, None, "1.0.82", ACP_ONLY
    ),
    trusted_agent!(
        "glm-acp-agent", "GLM Agent", Npx, "glm-acp-agent@1.8.0", "glm-acp-agent", &[], &[], &[],
        Node, None, "1.8.0", ACP_ONLY
    ),
    trusted_agent!(
        "goose", "Goose", ManagedBinary, "goose", "./goose-package\\goose.exe", &["acp"], &[], &[],
        Bundled, None, "1.48.0", ACP_ONLY
    ),
    trusted_agent!(
        "harn", "Harn", ManagedBinary, "harn", "harn.exe", &["serve", "acp"], &[], &[], Bundled,
        None, "0.10.128", ACP_ONLY
    ),
    trusted_agent!(
        "junie", "Junie", ManagedBinary, "junie", "./junie/junie.exe", &["--acp=true"], &[], &[],
        Bundled, None, "3123.3.0", ACP_ONLY
    ),
    trusted_agent!(
        "kilo", "Kilo", ManagedBinary, "kilo", "./kilo.exe", &["acp"], &[], &[], Bundled, None,
        "7.5.9", ACP_ONLY
    ),
    trusted_agent!(
        "minion-code", "Minion Code", Uvx, "minion-code==0.1.44", "minion-code", &["acp"], &[], &[],
        Python, Some("3.11"), "0.1.44", ACP_ONLY
    ),
    trusted_agent!(
        "mistral-vibe", "Mistral Vibe", ManagedBinary, "vibe-acp", "./vibe-acp.exe", &[], &[], &[],
        Bundled, None, "2.24.1", ACP_ONLY
    ),
    trusted_agent!(
        "nova", "Nova", Npx, "@compass-ai/nova@1.1.37", "nova", &["acp"], &[], &[], Node,
        Some("22.12.0"), "1.1.37", ACP_ONLY
    ),
    trusted_agent!(
        "poolside", "Poolside", ManagedBinary, "pool", "./pool-windows-amd64.exe", &["acp"], &[], &[],
        Bundled, None, "1.0.16", ACP_ONLY
    ),
    trusted_agent!(
        "qoder", "Qoder CLI", Npx, "@qoder-ai/qodercli@1.1.41", "qodercli", &["--acp"], &[], &[],
        Node, Some("20.0.0"), "1.1.41", ACP_ONLY
    ),
    trusted_agent!(
        "qwen-code", "Qwen Code", Npx, "@qwen-code/qwen-code@0.22.3", "qwen",
        &["--acp", "--experimental-skills"], &[], &[], Node, Some("22.0.0"), "0.22.3", ACP_ONLY
    ),
    trusted_agent!(
        "sigit", "siGit Code", ManagedBinary, "sigit", "./sigit-win-amd64.exe", &[], &[], &[], Bundled,
        None, "1.5.2", ACP_ONLY
    ),
    trusted_agent!(
        "stakpak", "Stakpak", ManagedBinary, "stakpak", "./stakpak.exe", &["acp"], &[], &[], Bundled,
        None, "0.3.88", ACP_ONLY
    ),
    trusted_agent!(
        "vtcode", "VT Code", ManagedBinary, "vtcode", "vtcode.exe", &["acp"],
        &[("VT_ACP_ENABLED", "1"), ("VT_ACP_ZED_ENABLED", "1")], &[], Bundled, None, "0.96.14", ACP_ONLY
    ),
];
