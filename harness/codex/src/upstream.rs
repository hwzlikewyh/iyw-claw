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
    release_ref: "rust-v0.152.1",
    tag_object: "3c6cfbab81e44218c729dc8c6b304cb760d1b8a1",
    commit: "5adb68a49933ae446bf11935662c83dba55a0804",
    protocol_components: &[
        "codex-rs/app-server",
        "codex-rs/app-server-client",
        "codex-rs/app-server-protocol",
    ],
};
