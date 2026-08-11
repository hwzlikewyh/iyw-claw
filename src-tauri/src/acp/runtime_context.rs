use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::acp::agent_storage::AgentStoragePaths;

pub fn prepend_tool_dirs(
    paths: Option<&AgentStoragePaths>,
    environment: &mut BTreeMap<String, String>,
) {
    let mut directories = crate::acp::builtin_agent_prompt::discover_tools(paths)
        .into_iter()
        .filter_map(|(_, path)| path)
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect::<Vec<_>>();
    if directories.is_empty() {
        return;
    }

    let (path_key, existing) = take_path(environment);
    directories.extend(std::env::split_paths(&existing));
    let mut seen = BTreeSet::new();
    directories.retain(|path| seen.insert(path_key_value(path)));
    let Ok(joined) = std::env::join_paths(directories) else {
        return;
    };
    environment.insert(path_key, joined.to_string_lossy().into_owned());
}

fn take_path(environment: &mut BTreeMap<String, String>) -> (String, String) {
    let keys = environment
        .keys()
        .filter(|key| key.eq_ignore_ascii_case("PATH"))
        .cloned()
        .collect::<Vec<_>>();
    let key = keys
        .first()
        .cloned()
        .unwrap_or_else(|| if cfg!(windows) { "Path" } else { "PATH" }.into());
    let value = keys
        .into_iter()
        .filter_map(|key| environment.remove(&key))
        .last()
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    (key, value)
}

fn path_key_value(path: &Path) -> String {
    let value = path.to_string_lossy();
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value.into_owned()
    }
}
