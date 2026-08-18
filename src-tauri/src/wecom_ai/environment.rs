use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{installer, runtime_root, CliPaths, WeComAiError, CONFIG_DIR_ENV, MANAGED_COMMAND_ENV};

pub fn inject_runtime_environment(
    data_dir: &Path,
    environment: &mut BTreeMap<String, String>,
) -> Result<(), WeComAiError> {
    let paths = CliPaths::new(data_dir);
    if !installer::launcher_path(&paths).is_file() {
        return Err(WeComAiError::Validation("managed launcher is missing"));
    }
    environment.insert(
        CONFIG_DIR_ENV.to_string(),
        paths.config.to_string_lossy().into_owned(),
    );
    environment.insert(
        MANAGED_COMMAND_ENV.to_string(),
        paths.command.to_string_lossy().into_owned(),
    );
    prepend_path(&paths.launcher, environment);
    Ok(())
}

pub fn inherit_terminal_environment(
    source: &BTreeMap<String, String>,
    target: &mut BTreeMap<String, String>,
) {
    for key in [CONFIG_DIR_ENV, MANAGED_COMMAND_ENV] {
        if let Some(value) = source.get(key).filter(|value| !value.is_empty()) {
            target.insert(key.to_string(), value.clone());
        }
    }
    reapply_runtime_path(target);
}

pub fn reapply_runtime_path(environment: &mut BTreeMap<String, String>) {
    let Some(config) = environment
        .get(CONFIG_DIR_ENV)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let Some(data_dir) = Path::new(config).parent().and_then(Path::parent) else {
        return;
    };
    prepend_path(&runtime_root(data_dir).join("bin"), environment);
}

fn prepend_path(directory: &Path, environment: &mut BTreeMap<String, String>) {
    let keys = environment
        .keys()
        .filter(|key| key.eq_ignore_ascii_case("PATH"))
        .cloned()
        .collect::<Vec<_>>();
    let path_key = keys
        .first()
        .cloned()
        .unwrap_or_else(|| if cfg!(windows) { "Path" } else { "PATH" }.into());
    let existing = keys
        .into_iter()
        .filter_map(|key| environment.remove(&key))
        .last()
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    let mut directories = vec![directory.to_path_buf()];
    directories.extend(std::env::split_paths(&existing));
    let mut seen = BTreeSet::new();
    directories.retain(|path| seen.insert(path_dedup_key(path)));
    if let Ok(joined) = std::env::join_paths(directories) {
        environment.insert(path_key, joined.to_string_lossy().into_owned());
    }
}

fn path_dedup_key(path: &Path) -> String {
    let value = path.to_string_lossy();
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value.into_owned()
    }
}
