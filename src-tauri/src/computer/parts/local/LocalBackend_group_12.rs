// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The helper backend. See the module note.
pub struct LocalBackend {
    pub(in crate::computer::local) slot: Mutex<Slot>,
    pub(in crate::computer::local) status: StdMutex<BackendStatus>,
    pub(in crate::computer::local) on_status: Box<dyn Fn(&BackendStatus) + Send + Sync>,
    /// Set after the cached driver was thrown away once for failing its
    /// checks, so a download that keeps failing them is reported rather than
    /// fetched again on every call.
    pub(in crate::computer::local) redownloaded: AtomicBool,
    /// The latest of the person's Stops this backend was told of. Kept here
    /// as well as in the helper, because the helper that heard it may not be
    /// the one an action reaches: with no helper running, the Stop reached
    /// nobody, and an action let through before it would start a fresh one.
    pub(in crate::computer::local) stopped: AtomicU64,
    /// Held for the whole of a [`close`](Self::close) or
    /// [`close_now`](Self::close_now), so that one returning means the helper
    /// another was already taking down is gone too.
    pub(in crate::computer::local) closing: Mutex<()>,
    /// The settings, when this backend follows them (see [`with_switch`]).
    ///
    /// [`with_switch`]: Self::with_switch
    pub(in crate::computer::local) switch:
        Option<crate::acp::computer_tools::ComputerToolsRuntimeConfig>,
}
