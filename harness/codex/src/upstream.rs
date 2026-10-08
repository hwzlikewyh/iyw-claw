//! Machine-readable identity for the Codex source this adapter targets.

/// Immutable upstream source identity used by build and diagnostics code.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct UpstreamPin {
    pub repository: &'static str,
    pub release_ref: &'static str,
    pub tag_object: &'static str,
    pub commit: &'static str,
    pub protocol_components: &'static [&'static str],
}

/// Keep this value synchronized with `upstream.lock`.
pub const UPSTREAM_PIN: UpstreamPin = UpstreamPin {
    repository: "https://github.com/openai/codex.git",
    release_ref: "rust-v0.161.0",
    tag_object: "7e21416b38834816c224ea0dfd135c3de94b2f15",
    commit: "979011409de0a60b52f179721948e65531d26144",
    protocol_components: &[
        "codex-rs/app-server",
        "codex-rs/app-server-client",
        "codex-rs/app-server-protocol",
    ],
};
