// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A read that passed steps 1–3, and what step 4 checks it against.
pub(super) struct Admitted {
    pub(in crate::commands::computer) ticket: ReadTicket,
    /// The switch-off count when the read was admitted.
    pub(in crate::commands::computer) switched_off: u64,
    /// The Stop count when the read was admitted.
    pub(in crate::commands::computer) stop: u64,
}
