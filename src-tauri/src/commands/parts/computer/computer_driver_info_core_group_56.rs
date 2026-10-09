// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// cua-driver as Settings shows it: the release this iyw-claw runs, what the
/// cache holds, and anything under way.
pub fn computer_driver_info_core(service: &ComputerService) -> DriverInfo {
    service.drivers.info()
}
