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
    pub(super) sha256: &'static str,
    pub(super) mirror_url: String,
    pub(super) official_url: String,
}

const NODE_VERSION_X64: &str = "24.0.0";
const NODE_VERSION_X86: &str = "22.23.1";
const GIT_VERSION: &str = "2.55.0.2";
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
            node_spec(
                NODE_VERSION_X64,
                "win-x64",
                "3d0fff80c87bb9a8d7f49f2f27832aa34a1477d137af46f5b14df5498be81304",
            ),
            git_spec(
                "64-bit",
                "e3ea2944cea4b3fabcd69c7c1669ef69b1b66c05ac7806d81224d0abad2dec31",
            ),
        )),
        "aarch64" => Some((
            node_spec(
                NODE_VERSION_X64,
                "win-arm64",
                "03b6676f4872fbe4645113de8e23da834a7c1464045369f2b7a374bf482a5e12",
            ),
            git_spec(
                "arm64",
                "0b2b81fdce284efd174cbb51b886ccea2fd271679c4b5c21f07d9e03bae51413",
            ),
        )),
        "x86" => Some((
            node_spec(
                NODE_VERSION_X86,
                "win-x86",
                "e298b368aad86c571447a3650db3ce19063373ffd39d6d73d014a5d9ad31dc62",
            ),
            git_spec(
                "32-bit",
                "04009f6150c1cec2d6779c51406c8c6a3f0133e57fa91c91eb8a030b93e68ccb",
            ),
        )),
        _ => None,
    }
}

fn node_spec(version: &'static str, platform: &'static str, sha256: &'static str) -> ComponentSpec {
    let asset = format!("node-v{version}-{platform}.zip");
    ComponentSpec {
        kind: ComponentKind::Node,
        version,
        mirror_url: format!("{NODE_MIRROR_BASE}/v{version}/{asset}"),
        official_url: format!("{NODE_OFFICIAL_BASE}/v{version}/{asset}"),
        asset,
        sha256,
    }
}

fn git_spec(asset_arch: &str, sha256: &'static str) -> ComponentSpec {
    let asset = format!("MinGit-{GIT_VERSION}-{asset_arch}.zip");
    ComponentSpec {
        kind: ComponentKind::Git,
        version: GIT_VERSION,
        mirror_url: format!("{GIT_MIRROR_BASE}/{GIT_RELEASE_TAG}/{asset}"),
        official_url: format!("{GIT_OFFICIAL_BASE}/{GIT_RELEASE_TAG}/{asset}"),
        asset,
        sha256,
    }
}
