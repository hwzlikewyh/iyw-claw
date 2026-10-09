// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Serializes every write of this record — the whole-record writer and the
/// group switch — for the reason the browser's lock exists: the keys are
/// upserted separately, and the status popover and the settings form are two
/// writers one click apart.
pub(super) static COMPUTER_TOOLS_WRITE_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));
