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
const NODE_VERSION_X64: &str = "24.20.0";
const NODE_VERSION_X86: &str = "22.23.2";
const NODE_SHA256_X64: &str = "6cac9ffbca8f6a47091e4b5c772e0606049c3871cb67d900c0cedde630e545ba";
const NODE_SHA256_ARM64: &str = "31c6799744de8a54601643098040c68c3697e56c94e407d61d0e5fa5f34191d7";
const NODE_SHA256_X86: &str = "725c9e2bdd1c2016b41c995a81f4fa36ce4e2ee565b7455d8f889182727df647";
const GIT_VERSION: &str = "2.55.0+windows.5";
const GIT_ASSET_VERSION: &str = "2.55.0.5";
const GIT_RELEASE_TAG: &str = "v2.55.0.windows.5";
const GIT_SHA256_X64: &str = "56d7b226b7693196cfc71fef26568f536c4a021ab6c37ff2db4287bed908e96e";
const GIT_SHA256_ARM64: &str = "05843f9d6e60306c3ab886799e2c67200caab921571f10512df3493049179ddb";
const GIT_SHA256_X86: &str = "2c5c030d18fc6a6437c6d3f85895302e67995507db740afb415648306c6b450d";
const UV_VERSION: &str = "0.12.9";
const UV_SHA256_X64: &str = "ddbfcee1ac615a0499f6aa97b5ec8ebdf3ee4a7714a48055ec2ba0030e3cf810";
const UV_SHA256_ARM64: &str = "d3360363a3cb671f2c854f4ef48cf4a57fe8664f8ec6a248076d68b797a8acc0";
const UV_SHA256_X86: &str = "62396154da2dc04a9fffb027e75ae3d971ca3ac7d3f0ffa7dd2c27c94798ce3f";

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
