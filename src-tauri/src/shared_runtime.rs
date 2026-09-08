//! 技能共用的工具、依赖和缓存路径；Agent 程序包仍由版本中心管理。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SHARED_TOOLS: [&str; 3] = ["node", "git", "uv"];

pub fn root() -> PathBuf {
    crate::paths::iyw_claw_user_dir().join("runtime")
}

pub fn tool_root(data_dir: &Path, tool: &str) -> PathBuf {
    if SHARED_TOOLS.contains(&tool) {
        root().join(tool)
    } else {
        data_dir.join("runtime").join(tool)
    }
}

pub fn envs_dir() -> PathBuf {
    root().join("envs")
}

pub fn uv_tools_dir() -> PathBuf {
    envs_dir().join("uv")
}

pub fn uv_bin_dir() -> PathBuf {
    root().join("uv").join("bin")
}

pub fn npm_prefix() -> PathBuf {
    envs_dir().join("npm")
}

pub fn environment() -> BTreeMap<&'static str, PathBuf> {
    let cache = root().join("cache");
    BTreeMap::from([
        (
            "IYW_CLAW_SKILLS_DIR",
            crate::paths::iyw_claw_user_dir().join("skills"),
        ),
        ("IYW_CLAW_SKILL_ENVS_DIR", envs_dir()),
        ("UV_CACHE_DIR", cache.join("uv")),
        ("UV_TOOL_DIR", uv_tools_dir()),
        ("UV_TOOL_BIN_DIR", uv_bin_dir()),
        ("UV_PYTHON_INSTALL_DIR", root().join("uv").join("python")),
        ("NPM_CONFIG_CACHE", cache.join("npm")),
        ("NPM_CONFIG_PREFIX", npm_prefix()),
    ])
}

pub fn bin_dirs() -> Vec<PathBuf> {
    let mut directories = SHARED_TOOLS
        .into_iter()
        .filter_map(crate::acp::version_center::managed_tool_executable)
        .filter_map(|path| path.parent().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    directories.push(uv_bin_dir());
    directories.push(crate::acp::npm_runtime::npm_prefix_bin_dir(&npm_prefix()));
    directories
}

pub fn prompt_context() -> String {
    environment()
        .into_iter()
        .map(|(name, path)| format!("- {name}: {}", path.display()))
        .collect::<Vec<_>>()
        .join("\n")
}
