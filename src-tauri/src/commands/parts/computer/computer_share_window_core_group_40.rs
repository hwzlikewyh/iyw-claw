// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Share one window at `level`, or stop sharing it at `none`.
pub async fn computer_share_window_core(
    service: &ComputerService,
    target_id: &str,
    level: GrantLevel,
) -> Result<Vec<SharedWindow>, AppCommandError> {
    let since = service.stop_count();
    let change = service.share_unless_stopped(target_id, level, since)?;
    service.announce(&change.into_iter().collect::<Vec<_>>());
    Ok(service.targets.shared())
}
