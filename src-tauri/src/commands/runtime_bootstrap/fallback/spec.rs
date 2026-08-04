#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ComponentKind {
    Node,
    Git,
    Uv,
}

impl ComponentKind {
    pub(super) fn tool_id(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Git => "git",
            Self::Uv => "uv",
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
    pub(super) expected_sha256: Option<&'static str>,
}

// Keep these aligned with the mirrored artifacts in iyw_fusion_api_component_artifacts.
// A pinned fallback that points at a version the mirror does not carry defeats the
// purpose of the fallback: it downloads a runtime the managed catalog can never match.
const NODE_VERSION_X64: &str = "24.18.1";
const NODE_VERSION_X86: &str = "22.23.1";
const GIT_VERSION: &str = "2.55.0+windows.3";
const GIT_ASSET_VERSION: &str = "2.55.0.3";
const GIT_RELEASE_TAG: &str = "v2.55.0.windows.3";
const UV_VERSION: &str = "0.12.1";
const UV_SHA256_X64: &str = "8fcb0cb46e1229065e344758980924e569bef5882ef45f46fada8fb24e06b74a";
const UV_SHA256_ARM64: &str = "9bc7c18e616230fa2dc6fb24bc3afde18a95c2b5c9433de747e9502c66041568";
const UV_SHA256_X86: &str = "9b51c33d307a8ab9e9dfd88d4ae1491761f63de0bffa3cec96bec536491c9b97";

const NODE_MIRROR_BASE: &str = "https://registry.npmmirror.com/-/binary/node";
const NODE_OFFICIAL_BASE: &str = "https://nodejs.org/dist";
const GIT_MIRROR_BASE: &str = "https://registry.npmmirror.com/-/binary/git-for-windows";
const GIT_OFFICIAL_BASE: &str = "https://github.com/git-for-windows/git/releases/download";
const UV_OFFICIAL_BASE: &str = "https://github.com/astral-sh/uv/releases/download";

pub(super) fn for_tool(tool_id: &str) -> Result<ComponentSpec, String> {
    if !cfg!(windows) {
        return Err("pinned runtime fallback is only available on Windows".to_string());
    }
    let (node, git, uv) = specs_for_current_arch().ok_or_else(|| {
        "pinned runtime fallback does not support this CPU architecture".to_string()
    })?;
    match tool_id {
        "node" => Ok(node),
        "git" => Ok(git),
        "uv" => Ok(uv),
        _ => Err(format!("no pinned fallback is registered for {tool_id}")),
    }
}

fn specs_for_current_arch() -> Option<(ComponentSpec, ComponentSpec, ComponentSpec)> {
    match std::env::consts::ARCH {
        "x86_64" => Some((
            node_spec(NODE_VERSION_X64, "win-x64"),
            git_spec("64-bit"),
            uv_spec("x86_64", UV_SHA256_X64),
        )),
        "aarch64" => Some((
            node_spec(NODE_VERSION_X64, "win-arm64"),
            git_spec("arm64"),
            uv_spec("aarch64", UV_SHA256_ARM64),
        )),
        "x86" => Some((
            node_spec(NODE_VERSION_X86, "win-x86"),
            git_spec("32-bit"),
            uv_spec("i686", UV_SHA256_X86),
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
        expected_sha256: None,
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
        expected_sha256: None,
        asset,
    }
}

fn uv_spec(asset_arch: &str, expected_sha256: &'static str) -> ComponentSpec {
    let asset = format!("uv-{asset_arch}-pc-windows-msvc.zip");
    let url = format!("{UV_OFFICIAL_BASE}/{UV_VERSION}/{asset}");
    ComponentSpec {
        kind: ComponentKind::Uv,
        version: UV_VERSION,
        mirror_url: url.clone(),
        official_url: url,
        expected_sha256: Some(expected_sha256),
        asset,
    }
}
