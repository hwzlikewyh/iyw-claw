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
    release_ref: "rust-v0.153.4",
    tag_object: "042fb41b7c813ac7999105e886b2b7aa715b5081",
    commit: "3d2ee51ca2d5db578f328aa75e20aa22c0197c9a",
    protocol_components: &[
        "codex-rs/app-server",
        "codex-rs/app-server-client",
        "codex-rs/app-server-protocol",
    ],
};
