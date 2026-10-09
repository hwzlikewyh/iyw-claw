// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A kqueue registered for one child's `NOTE_EXIT`.
pub(super) struct ExitWatch {
    pub(in crate::computer::spawn) kq: c_int,
    /// The registration found the child already gone (`ESRCH`): it is a
    /// zombie, there is nothing to wait for.
    pub(in crate::computer::spawn) exited: bool,
}
