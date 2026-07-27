use std::ffi::OsString;
use std::path::PathBuf;

pub(crate) fn claude_settings_path() -> PathBuf {
    super::claude::resolve_claude_config_dir().join("settings.json")
}

pub(crate) fn claude_mcp_config_path() -> PathBuf {
    claude_mcp_config_path_from(std::env::var_os("CLAUDE_CONFIG_DIR"), dirs::home_dir())
}

pub(crate) fn gemini_settings_path() -> PathBuf {
    gemini_settings_path_from(std::env::var_os("GEMINI_CLI_HOME"), dirs::home_dir())
}

pub(crate) fn openclaw_state_dir() -> PathBuf {
    openclaw_state_dir_from(
        std::env::var_os("OPENCLAW_STATE_DIR"),
        std::env::var_os("OPENCLAW_HOME"),
        dirs::home_dir(),
    )
}

pub(crate) fn openclaw_config_path() -> PathBuf {
    openclaw_state_dir().join("openclaw.json")
}

pub(crate) fn opencode_config_dir() -> PathBuf {
    opencode_config_dir_from(std::env::var_os("XDG_CONFIG_HOME"), dirs::home_dir())
}

pub(crate) fn cline_data_dir() -> PathBuf {
    cline_paths_from(std::env::var_os("CLINE_DIR"), dirs::home_dir()).0
}

pub(crate) fn cline_skills_dir() -> PathBuf {
    cline_paths_from(std::env::var_os("CLINE_DIR"), dirs::home_dir()).1
}

pub(crate) fn codebuddy_settings_path() -> PathBuf {
    super::codebuddy::resolve_codebuddy_config_dir().join("settings.json")
}

pub(crate) fn codebuddy_mcp_config_path() -> PathBuf {
    codebuddy_mcp_config_path_from(std::env::var_os("CODEBUDDY_CONFIG_DIR"), dirs::home_dir())
}

pub(crate) fn pi_agent_dir() -> PathBuf {
    pi_agent_dir_from(std::env::var_os("PI_CODING_AGENT_DIR"), dirs::home_dir())
}

fn claude_mcp_config_path_from(config_env: Option<OsString>, home: Option<PathBuf>) -> PathBuf {
    match nonempty_path(config_env) {
        Some(config) => config.join(".claude.json"),
        None => home.unwrap_or_default().join(".claude.json"),
    }
}

fn gemini_settings_path_from(cli_home: Option<OsString>, home: Option<PathBuf>) -> PathBuf {
    nonempty_path(cli_home)
        .unwrap_or_else(|| home.unwrap_or_default())
        .join(".gemini")
        .join("settings.json")
}

fn openclaw_state_dir_from(
    state_env: Option<OsString>,
    launcher_home_env: Option<OsString>,
    home: Option<PathBuf>,
) -> PathBuf {
    nonempty_path(state_env).unwrap_or_else(|| {
        nonempty_path(launcher_home_env)
            .or(home)
            .unwrap_or_default()
            .join(".openclaw")
    })
}

fn opencode_config_dir_from(xdg_config: Option<OsString>, home: Option<PathBuf>) -> PathBuf {
    nonempty_path(xdg_config)
        .or_else(|| home.map(|path| path.join(".config")))
        .unwrap_or_default()
        .join("opencode")
}

fn cline_paths_from(cline_dir: Option<OsString>, home: Option<PathBuf>) -> (PathBuf, PathBuf) {
    if let Some(custom) = nonempty_path(cline_dir) {
        let skills = custom.join("skills");
        return (custom, skills);
    }
    let root = home.unwrap_or_default().join(".cline");
    (root.join("data"), root.join("skills"))
}

fn codebuddy_mcp_config_path_from(config_env: Option<OsString>, home: Option<PathBuf>) -> PathBuf {
    match nonempty_path(config_env) {
        Some(config) => config.join(".codebuddy.json"),
        None => home.unwrap_or_default().join(".codebuddy.json"),
    }
}

fn pi_agent_dir_from(agent_env: Option<OsString>, home: Option<PathBuf>) -> PathBuf {
    nonempty_path(agent_env).unwrap_or_else(|| home.unwrap_or_default().join(".pi").join("agent"))
}

fn nonempty_path(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|item| !item.is_empty()).map(PathBuf::from)
}

