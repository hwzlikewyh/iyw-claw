use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn resolve_grok_home_dir() -> PathBuf {
    resolve_grok_home_from(std::env::var_os("GROK_HOME"), dirs::home_dir())
}

pub(super) fn resolve_grok_home_from(
    grok_home_env: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> PathBuf {
    grok_home_env
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir.unwrap_or_default().join(".grok"))
}

pub(super) fn read_subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honors_grok_home_env() {
        let home = resolve_grok_home_from(Some("/custom/grok".into()), Some("/home/me".into()));
        assert_eq!(home, PathBuf::from("/custom/grok"));
        let fallback = resolve_grok_home_from(None, Some("/home/me".into()));
        assert_eq!(fallback, PathBuf::from("/home/me/.grok"));
    }
}
