// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How often lapsed grants are swept, so the panel shows a window as no
/// longer shared when its time runs out rather than at the next read.
pub(super) const EXPIRY_SWEEP: Duration = Duration::from_secs(30);

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
