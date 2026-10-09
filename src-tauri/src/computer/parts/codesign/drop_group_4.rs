// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` came from a Copy/Create call and is released
            // exactly once, here.
            unsafe { CFRelease(self.0) };
        }
    }
}
