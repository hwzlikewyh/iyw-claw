#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ComponentKind {
    Node,
    Git,
}

impl ComponentKind {
    pub(super) fn tool_id(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Git => "git",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ComponentSpec {
    pub(super) kind: ComponentKind,
    pub(super) version: &'static str,
    pub(super) asset: String,
    pub(super) mirror_url: String,
    pub(super) official_url: String,
}

const NODE_VERSION_X64: &str = "24.0.0";
const NODE_VERSION_X86: &str = "22.23.1";
const GIT_VERSION: &str = "2.55.0+windows.2";
const GIT_ASSET_VERSION: &str = "2.55.0.2";
const GIT_RELEASE_TAG: &str = "v2.55.0.windows.2";

const NODE_MIRROR_BASE: &str = "https://registry.npmmirror.com/-/binary/node";
const NODE_OFFICIAL_BASE: &str = "https://nodejs.org/dist";
const GIT_MIRROR_BASE: &str = "https://registry.npmmirror.com/-/binary/git-for-windows";
const GIT_OFFICIAL_BASE: &str = "https://github.com/git-for-windows/git/releases/download";

pub(super) fn for_tool(tool_id: &str) -> Result<ComponentSpec, String> {
    if !cfg!(windows) {
        return Err("pinned runtime fallback is only available on Windows".to_string());
    }
    let (node, git) = specs_for_current_arch().ok_or_else(|| {
        "pinned runtime fallback does not support this CPU architecture".to_string()
    })?;
    match tool_id {
        "node" => Ok(node),
        "git" => Ok(git),
        _ => Err(format!("no pinned fallback is registered for {tool_id}")),
    }
}

fn specs_for_current_arch() -> Option<(ComponentSpec, ComponentSpec)> {
    match std::env::consts::ARCH {
        "x86_64" => Some((
            node_spec(NODE_VERSION_X64, "win-x64"),
            git_spec("64-bit"),
        )),
        "aarch64" => Some((
            node_spec(NODE_VERSION_X64, "win-arm64"),
            git_spec("arm64"),
        )),
        "x86" => Some((
            node_spec(NODE_VERSION_X86, "win-x86"),
            git_spec("32-bit"),
        )),
        _ => None,
    }
}

fn node_spec(version: &'static str, platform: &'static str) -> ComponentSpec {
    let asset = format!("node-v{version}-{platform}.zip");
    ComponentSpec {
        kind: ComponentKind::Node,
        version,
        mirror_url: format!("{NODE_MIRROR_BASE}/v{version}/{asset}"),
        official_url: format!("{NODE_OFFICIAL_BASE}/v{version}/{asset}"),
        asset,
    }
}

fn git_spec(asset_arch: &str) -> ComponentSpec {
    let asset = format!("MinGit-{GIT_ASSET_VERSION}-{asset_arch}.zip");
    ComponentSpec {
        kind: ComponentKind::Git,
        version: GIT_VERSION,
        mirror_url: format!("{GIT_MIRROR_BASE}/{GIT_RELEASE_TAG}/{asset}"),
        official_url: format!("{GIT_OFFICIAL_BASE}/{GIT_RELEASE_TAG}/{asset}"),
        asset,
    }
}
