// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Shared, hot-swappable handle to [`ComputerToolsConfig`]. Cloned into
/// `DelegationInjection` (read at injection), into the access impl (read at
/// call time) and into `AppState` (updated on save).
///
/// Unlike the browser's handle it can also be watched: switching computer use
/// off ends every grant and stops the helper, and whoever owns those — the
/// desktop's computer service — learns of the switch here, whichever of the
/// three writers (settings form, status popover, web settings) moved it.
///
/// Two ways to learn of it, for two kinds of consequence. What a change takes
/// away — grants — goes through [`on_change`](Self::on_change): run inside
/// [`set`](Self::set), once per change, with the settings before and after,
/// so it is done before the write returns and no change is ever merged into
/// the next. What can wait for a task to be scheduled — stopping and starting
/// the helper — goes through [`subscribe`](Self::subscribe), which sees only
/// the latest value (hence `switched_off`).
#[derive(Clone)]
pub struct ComputerToolsRuntimeConfig {
    pub(in crate::acp::computer_tools) inner: Arc<RwLock<ComputerToolsConfig>>,
    pub(in crate::acp::computer_tools) changes:
        Arc<tokio::sync::watch::Sender<ComputerToolsConfig>>,
    pub(in crate::acp::computer_tools) hook: Arc<std::sync::RwLock<Option<ChangeHook>>>,
    /// Whether this process serves computer use at all (see
    /// [`mark_served`](Self::mark_served)).
    pub(in crate::acp::computer_tools) served: Arc<std::sync::atomic::AtomicBool>,
}
