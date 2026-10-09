// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The running helper, and whether one may be started.
pub(super) struct Slot {
    pub(in crate::computer::local) connection: Option<Arc<Connection>>,
    /// Closed while computer use is switched off: no helper is started, so a
    /// read that was admitted just before the switch cannot bring one back.
    pub(in crate::computer::local) open: bool,
}
