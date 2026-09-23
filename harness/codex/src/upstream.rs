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
    release_ref: "rust-v0.156.1",
    tag_object: "81e8e29b2956dfe9b092c63953a9ed282781e77c",
    commit: "b412ff32c417f855c2b2d1581b77058eed87c84b",
    protocol_components: &[
        "codex-rs/app-server",
        "codex-rs/app-server-client",
        "codex-rs/app-server-protocol",
    ],
};
