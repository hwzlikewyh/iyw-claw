// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, McpError>>>>>;

pub(super) fn fail_pending(pending: &Pending) {
    let waiting: Vec<_> = pending
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .drain()
        .map(|(_, tx)| tx)
        .collect();
    for tx in waiting {
        let _ = tx.send(Err(McpError::Closed));
    }
}
