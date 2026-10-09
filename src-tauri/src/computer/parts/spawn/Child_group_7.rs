// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A spawned child. Signals are sent only while the child is known to be
/// unreaped; exit is observed by a waiter thread that first waits for the exit
/// *without* reaping (a kqueue `NOTE_EXIT`, which fires when the child becomes
/// a zombie), then collects it under the lock with a `waitpid` that does not
/// block — so there is no moment at which this handle still believes in a pid
/// the kernel has already handed to someone else, and the lock is never held
/// while the child runs.
///
/// The watch is set up before the handle exists. A child whose exit could not
/// be watched is killed and collected on the spot rather than handed out: a
/// handle that cannot see its child exit could neither signal it safely nor
/// know when to stop.
#[derive(Debug, Clone)]
pub struct Child {
    pub(in crate::computer::spawn) pid: u32,
    pub(in crate::computer::spawn) state: Arc<Mutex<ChildState>>,
    pub(in crate::computer::spawn) exited: tokio::sync::watch::Receiver<bool>,
}
