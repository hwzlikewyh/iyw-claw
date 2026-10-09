// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether the stop shortcut is in force — the same status
/// `computer://stop-key` carries when it changes. Empty where there is none
/// (iyw-claw-server).
pub fn computer_stop_key_status_core(service: &ComputerService) -> StopKeyStatus {
    service.stop_key_status()
}
