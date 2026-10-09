// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The shared windows — iyw-claw's own state, with no helper to start, for a
/// window that has just loaded.
pub fn computer_shared_state_core(service: &ComputerService) -> SharedState {
    service.shared_state()
}
