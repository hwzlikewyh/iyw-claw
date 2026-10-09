// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// See the module note.
#[derive(Default)]
pub struct TargetTable {
    pub(in crate::computer::targets) inner: Mutex<Inner>,
}
