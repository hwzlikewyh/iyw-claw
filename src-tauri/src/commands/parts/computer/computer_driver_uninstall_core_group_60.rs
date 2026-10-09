// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Remove cua-driver: computer use goes off, the helper stops, every cached
/// release goes.
pub async fn computer_driver_uninstall_core(
    service: &ComputerService,
    conn: &sea_orm::DatabaseConnection,
) -> Result<DriverInfo, AppCommandError> {
    service.uninstall_driver(conn).await
}
