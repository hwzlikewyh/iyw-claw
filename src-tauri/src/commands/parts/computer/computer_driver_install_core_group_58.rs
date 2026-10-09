// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Fetch the release this iyw-claw runs, and clear older ones. Progress travels
/// on `computer://driver`.
pub async fn computer_driver_install_core(
    service: &ComputerService,
) -> Result<DriverInfo, AppCommandError> {
    service
        .drivers
        .install()
        .await
        .map_err(AppCommandError::configuration_invalid)
}
