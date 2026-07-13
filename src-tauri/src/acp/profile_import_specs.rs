use std::path::{Path, PathBuf};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::profile_import::{
    import_entry, ProfileImportEntry, ProfileImportSpec, ProfileSourceRoots,
};
use crate::models::agent::AgentType;

const CLAUDE: &[&str] = &[
    "settings.json",
    "settings.local.json",
    ".credentials.json",
    "credentials.json",
    "skills",
    "commands",
    "hooks",
    "rules",
    "CLAUDE.md",
];
const CODEX: &[&str] = &[
    "config.toml",
    "auth.json",
    "AGENTS.md",
    "skills",
    "prompts",
    "rules",
];
const GEMINI: &[&str] = &[
    "settings.json",
    ".env",
    "oauth_creds.json",
    "google_accounts.json",
    "mcp-oauth-tokens.json",
    "skills",
    "commands",
    "extensions",
    "policies",
    "trustedFolders.json",
];
const OPENCLAW: &[&str] = &[
    "openclaw.json",
    "clawdbot.json",
    ".env",
    "skills",
    "extensions",
    "credentials",
    "identity",
    "exec-approvals.json",
    "mcp.json",
    "secrets.json",
];
const OPENCODE_CONFIG: &[&str] = &[
    "opencode.json",
    "config.json",
    "skills",
    "commands",
    "agents",
    "themes",
];
const OPENCODE_DATA: &[&str] = &["auth.json", "credentials.json", "mcp-auth.json"];
const CLINE_DATA: &[&str] = &["globalState.json", "secrets.json", "settings", "auth"];
const HERMES: &[&str] = &[
    "config.yaml",
    ".env",
    "auth.json",
    ".anthropic_oauth.json",
    "google_token.json",
    "credentials",
    "auth",
    "skills",
    "mcp.json",
    "mcp-oauth",
    "shell-hooks-allowlist.json",
    "toolsets",
    "prompts",
    "hooks",
];
const CODEBUDDY: &[&str] = &[
    "settings.json",
    "mcp.json",
    "auth.json",
    "credentials",
    "skills",
    "commands",
    "hooks",
    "rules",
];
const KIMI: &[&str] = &[
    "config.toml",
    "mcp.json",
    "credentials",
    "oauth",
    "skills",
    "agents",
    "commands",
    "AGENTS.md",
];
const PI: &[&str] = &[
    "settings.json",
    "auth.json",
    "models.json",
    "skills",
    "extensions",
    "prompts",
    "themes",
    "packages",
];

pub(super) fn build_profile_import_specs(
    paths: &AgentStoragePaths,
    sources: &ProfileSourceRoots,
) -> Vec<ProfileImportSpec> {
    vec![
        claude_spec(paths, sources),
        codex_spec(paths, sources),
        direct_spec(paths, sources, AgentType::Gemini, GEMINI),
        direct_spec(paths, sources, AgentType::OpenClaw, OPENCLAW),
        opencode_spec(paths, sources),
        cline_spec(paths, sources),
        direct_spec(paths, sources, AgentType::Hermes, HERMES),
        codebuddy_spec(paths, sources),
        direct_spec(paths, sources, AgentType::KimiCode, KIMI),
        direct_spec(paths, sources, AgentType::Pi, PI),
    ]
}

fn claude_spec(paths: &AgentStoragePaths, sources: &ProfileSourceRoots) -> ProfileImportSpec {
    let mut spec = direct_spec(paths, sources, AgentType::ClaudeCode, CLAUDE);
    spec.entries
        .push(absolute_entry(sources.claude_mcp_path(), ".claude.json"));
    spec
}

fn codex_spec(paths: &AgentStoragePaths, sources: &ProfileSourceRoots) -> ProfileImportSpec {
    let mut spec = direct_spec(paths, sources, AgentType::Codex, CODEX);
    add_shared_skills(&mut spec.entries, sources, "skills");
    spec
}

fn opencode_spec(paths: &AgentStoragePaths, sources: &ProfileSourceRoots) -> ProfileImportSpec {
    let mut entries = named_entries(
        &sources.xdg_config.join("opencode"),
        "config/opencode",
        OPENCODE_CONFIG,
    );
    entries.extend(named_entries(
        &sources.xdg_data.join("opencode"),
        "data/opencode",
        OPENCODE_DATA,
    ));
    add_shared_skills(&mut entries, sources, "config/opencode/skills");
    profile_spec(paths, AgentType::OpenCode, entries)
}

fn cline_spec(paths: &AgentStoragePaths, sources: &ProfileSourceRoots) -> ProfileImportSpec {
    let mut entries = named_entries(&sources.profile(AgentType::Cline), "", CLINE_DATA);
    entries.push(absolute_entry(sources.cline_skills_dir(), "skills"));
    add_shared_skills(&mut entries, sources, "skills");
    profile_spec(paths, AgentType::Cline, entries)
}

fn codebuddy_spec(paths: &AgentStoragePaths, sources: &ProfileSourceRoots) -> ProfileImportSpec {
    let mut spec = direct_spec(paths, sources, AgentType::CodeBuddy, CODEBUDDY);
    spec.entries.push(absolute_entry(
        sources.codebuddy_mcp_path(),
        ".codebuddy.json",
    ));
    spec
}

fn direct_spec(
    paths: &AgentStoragePaths,
    sources: &ProfileSourceRoots,
    agent_type: AgentType,
    names: &[&str],
) -> ProfileImportSpec {
    profile_spec(
        paths,
        agent_type,
        named_entries(&sources.profile(agent_type), "", names),
    )
}

fn profile_spec(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    entries: Vec<ProfileImportEntry>,
) -> ProfileImportSpec {
    ProfileImportSpec {
        agent_type,
        destination_root: paths.profile(agent_type).root,
        entries,
    }
}

fn named_entries(source: &Path, destination: &str, names: &[&str]) -> Vec<ProfileImportEntry> {
    names
        .iter()
        .map(|name| {
            let target = if destination.is_empty() {
                PathBuf::from(name)
            } else {
                Path::new(destination).join(name)
            };
            import_entry(source, name, target)
        })
        .collect()
}

fn absolute_entry(source: &Path, destination: &str) -> ProfileImportEntry {
    let source_root = source
        .parent()
        .expect("resolved profile entry has a parent");
    let source_name = source
        .file_name()
        .expect("resolved profile entry has a name");
    import_entry(source_root, source_name, destination)
}

fn add_shared_skills(
    entries: &mut Vec<ProfileImportEntry>,
    sources: &ProfileSourceRoots,
    destination: &str,
) {
    entries.push(import_entry(
        &sources.home.join(".agents"),
        "skills",
        destination,
    ));
}
