// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Longest line accepted from the driver. A window screenshot rides inside
/// one line as base64, so the bound is generous; it exists so a driver gone
/// wrong cannot make the helper buffer without end.
pub(super) const MAX_LINE_BYTES: usize = 64 * 1024 * 1024;

/// How long one message may take to go out. The driver reads its input as it
/// comes, so a write that cannot finish means a driver that stopped reading:
/// wedged. Half a message on the pipe cannot be taken back, so the client is
/// closed and the next call starts another driver.
pub(super) const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
