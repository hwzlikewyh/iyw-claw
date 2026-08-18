use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Resolve the DeepSeek Harness session directory from its documented overrides.
pub(crate) fn resolve_deepseek_sessions_root() -> PathBuf {
    resolve_sessions_root_from(
        std::env::var_os("DEEPSEEK_ACP_SESSIONS_ROOT"),
        std::env::var_os("DSH_HOME"),
        dirs::home_dir(),
    )
}

pub(crate) fn resolve_dsh_home_dir() -> PathBuf {
    resolve_dsh_home_from(std::env::var_os("DSH_HOME"), dirs::home_dir())
}

pub(crate) fn resolve_dsh_agents_home_dir() -> PathBuf {
    resolve_dsh_agents_home_from(std::env::var_os("DSH_AGENTS_HOME"), dirs::home_dir())
}

fn resolve_sessions_root_from(
    sessions_root: Option<OsString>,
    dsh_home: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> PathBuf {
    sessions_root
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| resolve_dsh_home_from(dsh_home, home_dir).join("sessions"))
}

fn resolve_dsh_home_from(dsh_home: Option<OsString>, home_dir: Option<PathBuf>) -> PathBuf {
    dsh_home
        .filter(|value| !value.to_string_lossy().trim().is_empty())
        .map(|value| expand_home_prefix(&value.to_string_lossy(), home_dir.as_ref()))
        .unwrap_or_else(|| home_dir.unwrap_or_default().join(".dsh"))
}

fn resolve_dsh_agents_home_from(
    agents_home: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> PathBuf {
    agents_home
        .filter(|value| !value.to_string_lossy().trim().is_empty())
        .map(|value| expand_home_prefix(&value.to_string_lossy(), home_dir.as_ref()))
        .unwrap_or_else(|| home_dir.unwrap_or_default().join(".agents"))
}

fn expand_home_prefix(value: &str, home_dir: Option<&PathBuf>) -> PathBuf {
    let Some(home) = home_dir else {
        return PathBuf::from(value);
    };
    if value == "~" {
        return home.clone();
    }
    value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
        .map(|rest| home.join(rest))
        .unwrap_or_else(|| PathBuf::from(value))
}

pub(super) fn read_subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}
