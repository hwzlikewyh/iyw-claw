use std::ffi::OsStr;
use std::io;

use rmcp::model::ProtocolVersion;
use rmcp::service::ClientLifecycleMode;

/// MCP compatibility policy selected once when a Codex session is created.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum McpProtocolMode {
    /// Preserve the existing MCP initialization and OAuth behavior.
    #[default]
    Legacy,
    /// Allow the MCP 2026-07-28 discovery and request lifecycle.
    V20260728,
    /// Probe an eligible local stdio server even when its launch config omits a version.
    DiscoverLocalStdio,
}

impl McpProtocolMode {
    /// Returns the newest protocol version this compatibility policy can use.
    pub fn preferred_protocol_version(self) -> ProtocolVersion {
        match self {
            Self::Legacy => ProtocolVersion::V_2025_06_18,
            Self::V20260728 | Self::DiscoverLocalStdio => ProtocolVersion::V_2026_07_28,
        }
    }

    pub(crate) fn client_lifecycle(self) -> ClientLifecycleMode {
        match self {
            Self::Legacy => ClientLifecycleMode::Initialize,
            Self::V20260728 | Self::DiscoverLocalStdio => ClientLifecycleMode::Auto {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                legacy_version: Some(ProtocolVersion::V_2025_06_18),
            },
        }
    }

    /// Selects the stdio handshake from the client policy and the server's declared version.
    pub fn stdio_mode(self, requested_version: Option<&OsStr>) -> io::Result<Self> {
        match (self, requested_version) {
            (Self::Legacy, _) => Ok(Self::Legacy),
            (Self::DiscoverLocalStdio, None) => Ok(Self::DiscoverLocalStdio),
            (Self::V20260728, None) => Ok(Self::Legacy),
            (_, Some(version)) if version == OsStr::new("2026-07-28") => Ok(Self::V20260728),
            (_, Some(version)) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "unsupported CODEX_MCP_PROTOCOL_VERSION `{}` for stdio MCP server; expected `2026-07-28`",
                    version.to_string_lossy()
                ),
            )),
        }
    }
}
