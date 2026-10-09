// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What must hold at the moment an action goes out: no Stop has come since
/// iyw-claw let it through, the pid is still the process the window was shared
/// from (a relaunch under a reused pid is another process, whose windows
/// nobody shared), so is the process drawing inside it where that is another
/// (a frame's application, relaunched into the same frame, is a window nobody
/// shared), and the session is affirmatively unlocked and on this console.
/// Asked just before each driver call — and, for the change the helper makes
/// itself through Accessibility, on the thread that makes it, after
/// everything read to decide on it: owned for that.
#[derive(Clone)]
pub struct Delivery {
    pub(in crate::computer::helper) stopped: Arc<AtomicU64>,
    pub(in crate::computer::helper) stop: u64,
    pub(in crate::computer::helper) pid: u32,
    pub(in crate::computer::helper) window_id: u64,
    pub(in crate::computer::helper) started_at: u64,
    pub(in crate::computer::helper) content: Option<ProcessRun>,
}
