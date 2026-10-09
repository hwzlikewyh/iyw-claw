// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The protocol revision the helper asks for. The driver speaks this one and
/// a newer one; this is the older, stabler of the two.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
