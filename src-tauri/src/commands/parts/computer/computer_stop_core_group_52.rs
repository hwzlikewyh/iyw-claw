// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Stop: every grant ended, whatever is under way cut off, the driver killed
/// mid-action. Answers once all three are done. Nothing is held after it.
pub async fn computer_stop_core(service: &ComputerService) {
    service.stop().await;
}
