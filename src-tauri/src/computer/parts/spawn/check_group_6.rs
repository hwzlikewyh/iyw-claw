// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// `posix_spawn` and friends return the error number instead of setting
/// `errno`.
pub(super) fn check(rc: c_int) -> std::io::Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(rc))
    }
}

#[derive(Debug, Default)]
pub(super) struct ChildState {
    /// Set in the same critical section as the reap. Until the reap, the
    /// kernel keeps the pid reserved (a zombie), so a signal sent under this
    /// lock while this is `false` can only ever reach this child.
    pub(in crate::computer::spawn) reaped: bool,
    pub(in crate::computer::spawn) exit_status: Option<i32>,
}
