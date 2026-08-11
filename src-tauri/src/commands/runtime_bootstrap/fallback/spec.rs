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
    /// Accelerated sources, best first, all tried before [`Self::official_url`].
    /// May be empty, in which case only the official source is used.
    pub(super) mirror_urls: Vec<String>,
    pub(super) official_url: String,
    pub(super) expected_sha256: Option<&'static str>,
}

impl ComponentSpec {
    /// Ordered download sources: every mirror first, the official upstream last.
    ///
    /// The returned labels are telemetry values. The first mirror keeps the
    /// label `mirror` and the upstream keeps `official`, so log queries written
    /// against the previous single-mirror shape keep matching; additional
    /// mirrors are `mirror-2`, `mirror-3`, ...
    pub(super) fn sources(&self) -> Vec<(String, &str)> {
        let mut sources: Vec<(String, &str)> = self
            .mirror_urls
            .iter()
            .enumerate()
            .map(|(index, url)| {
                let label = if index == 0 {
                    "mirror".to_string()
                } else {
                    format!("mirror-{}", index + 1)
                };
                (label, url.as_str())
            })
            .collect();
        sources.push(("official".to_string(), self.official_url.as_str()));
        sources
    }
}

// Keep these aligned with the mirrored artifacts in iyw_fusion_api_component_artifacts.
// A pinned fallback that points at a version the mirror does not carry defeats the
// purpose of the fallback: it downloads a runtime the managed catalog can never match.
const NODE_VERSION_X64: &str = "24.19.0";
const NODE_VERSION_X86: &str = "22.23.1";
const NODE_SHA256_X64: &str = "57f71ab3652e797d84acddc79c81cc9ff1c6ddb2a1974cdb83f00fee9bff4c73";
const NODE_SHA256_ARM64: &str = "8502f4a50b458d4cc38ed8f2001556c2cd239d464920f74017926ccb1e1c157f";
const NODE_SHA256_X86: &str = "e298b368aad86c571447a3650db3ce19063373ffd39d6d73d014a5d9ad31dc62";
const GIT_VERSION: &str = "2.55.0+windows.3";
const GIT_ASSET_VERSION: &str = "2.55.0.3";
const GIT_RELEASE_TAG: &str = "v2.55.0.windows.3";
const GIT_SHA256_X64: &str = "f48e2d2dc74a24454adc6d8fd0ac25bf9c2386f19cfb06202b9465aaad4f9f05";
const GIT_SHA256_ARM64: &str = "f7748965d5068e81ad93ca1923650db6742d6e22332b1ae7567a841c59f6bde5";
const GIT_SHA256_X86: &str = "352380d06caa45e569a3b3967b6d1d6c605d564c29f37ef059b59e657a522ef4";
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
            node_spec(NODE_VERSION_X64, "win-x64", NODE_SHA256_X64),
            git_spec("64-bit", GIT_SHA256_X64),
            uv_spec("x86_64", UV_SHA256_X64),
        )),
        "aarch64" => Some((
            node_spec(NODE_VERSION_X64, "win-arm64", NODE_SHA256_ARM64),
            git_spec("arm64", GIT_SHA256_ARM64),
            uv_spec("aarch64", UV_SHA256_ARM64),
        )),
        "x86" => Some((
            node_spec(NODE_VERSION_X86, "win-x86", NODE_SHA256_X86),
            git_spec("32-bit", GIT_SHA256_X86),
            uv_spec("i686", UV_SHA256_X86),
        )),
        _ => None,
    }
}

fn node_spec(
    version: &'static str,
    platform: &'static str,
    expected_sha256: &'static str,
) -> ComponentSpec {
    let asset = format!("node-v{version}-{platform}.zip");
    // Node ships from nodejs.org, not GitHub, so gh-proxy does not apply here —
    // npmmirror is already the mainland-friendly source.
    ComponentSpec {
        kind: ComponentKind::Node,
        version,
        mirror_urls: vec![format!("{NODE_MIRROR_BASE}/v{version}/{asset}")],
        official_url: format!("{NODE_OFFICIAL_BASE}/v{version}/{asset}"),
        expected_sha256: Some(expected_sha256),
        asset,
    }
}

fn git_spec(asset_arch: &str, expected_sha256: &'static str) -> ComponentSpec {
    let asset = format!("MinGit-{GIT_ASSET_VERSION}-{asset_arch}.zip");
    let official_url = format!("{GIT_OFFICIAL_BASE}/{GIT_RELEASE_TAG}/{asset}");
    // npmmirror stays first — it is a real CDN, not a volunteer proxy. The
    // gh-proxies sit between it and direct GitHub so a stale npmmirror still has
    // several accelerated routes to try before the slow path.
    let mut mirror_urls = vec![format!("{GIT_MIRROR_BASE}/{GIT_RELEASE_TAG}/{asset}")];
    mirror_urls.extend(crate::github_mirror::mirror_urls(&official_url));
    ComponentSpec {
        kind: ComponentKind::Git,
        version: GIT_VERSION,
        mirror_urls,
        official_url,
        expected_sha256: Some(expected_sha256),
        asset,
    }
}

fn uv_spec(asset_arch: &str, expected_sha256: &'static str) -> ComponentSpec {
    let asset = format!("uv-{asset_arch}-pc-windows-msvc.zip");
    let official_url = format!("{UV_OFFICIAL_BASE}/{UV_VERSION}/{asset}");
    // uv has no first-party mainland mirror, so gh-proxy is the only
    // acceleration available. Previously this slot duplicated the GitHub URL,
    // which made the "mirror" attempt a second identical direct request.
    ComponentSpec {
        kind: ComponentKind::Uv,
        version: UV_VERSION,
        mirror_urls: crate::github_mirror::mirror_urls(&official_url),
        official_url,
        expected_sha256: Some(expected_sha256),
        asset,
    }
}
