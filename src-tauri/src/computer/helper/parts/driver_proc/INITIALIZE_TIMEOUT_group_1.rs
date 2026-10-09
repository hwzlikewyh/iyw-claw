// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How long the driver has to finish the MCP handshake.
pub(super) const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(20);

/// How long the driver has to report its configuration after the handshake.
pub(super) const CONFIG_TIMEOUT: Duration = Duration::from_secs(10);

/// The driver's idle-session timeout, in seconds: ten years, which is to say
/// never. The driver treats `0` as "use the default" (five minutes).
pub(super) const SESSION_IDLE_TTL_SECS: &str = "315360000";

/// The driver's configuration file, relative to its home: captures at the
/// window's own size (`0` is "no limit"). See the module note.
pub(super) const DRIVER_CONFIG: &[u8] = br#"{"max_image_dimension":0}"#;
