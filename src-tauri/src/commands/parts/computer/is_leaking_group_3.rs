// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl HostTccStatus {
    /// iyw-claw itself has been granted a permission that every agent's shell
    /// inherits.
    pub fn is_leaking(&self) -> bool {
        self.self_responsible && (self.accessibility || self.screen_recording)
    }
}
