// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Share an application as a whole at `level`, or end its share at `none`:
/// the application `target_id` is a window of, or the one shared as
/// `app_id`. Answers with what is shared now.
pub async fn computer_share_app_core(
    service: &ComputerService,
    target_id: Option<&str>,
    app_id: Option<&str>,
    level: GrantLevel,
) -> Result<SharedState, AppCommandError> {
    let target = match (target_id, app_id) {
        (_, Some(app_id)) => AppTarget::Share(app_id),
        (Some(target_id), None) => AppTarget::Window(target_id),
        (None, None) => {
            return Err(AppCommandError::configuration_invalid(
                "name the application by one of its windows or by its share",
            ))
        }
    };
    let since = service.stop_count();
    let change = service.share_app_unless_stopped(target, level, since)?;
    service.announce_change(change);
    Ok(service.shared_state())
}
