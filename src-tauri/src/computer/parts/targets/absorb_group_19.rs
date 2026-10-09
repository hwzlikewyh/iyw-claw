// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl AppChange {
    pub fn absorb(&mut self, other: AppChange) {
        self.windows.extend(other.windows);
        self.app_changed |= other.app_changed;
    }
}
