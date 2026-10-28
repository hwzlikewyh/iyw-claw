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
    release_ref: "rust-v0.165.0",
    tag_object: "e374836b2728d9b7aa981c2902649e640a2edaa8",
    commit: "360319d8dee62d87e2d59fc0c21805d79519cfed",
    protocol_components: &[
        "codex-rs/app-server",
        "codex-rs/app-server-client",
        "codex-rs/app-server-protocol",
    ],
};
