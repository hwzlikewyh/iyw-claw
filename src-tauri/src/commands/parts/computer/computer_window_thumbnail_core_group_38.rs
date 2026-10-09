// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A small picture of one window for the picker, as a `data:` URL. Never for
/// a window that can never be shared — there is no decision to make about it
/// — nor for a minimized one, or one whose application is hidden, which shows
/// nothing to capture (the helper refuses one it finds so since the list was
/// read).
pub async fn computer_window_thumbnail_core(
    service: &ComputerService,
    target_id: &str,
) -> Result<Option<String>, AppCommandError> {
    let config = service.config.snapshot().await;
    let Some(entry) = service.targets.get(target_id) else {
        return Ok(None);
    };
    if !config.enabled
        || entry.gone
        || entry.minimized == Some(true)
        || entry.hidden == Some(true)
        || grantable(&entry.app, &service.me, &blocklist_of(&config)).is_err()
    {
        return Ok(None);
    }
    let _turn = service.turn.lock().await;
    match service
        .backend
        .capture(entry.identity.pid, entry.identity.window_id, Some(480))
        .await
    {
        Ok(raw) => Ok(Some(format!("data:image/png;base64,{}", raw.png_base64))),
        Err(BackendError::PermissionMissing(_))
        | Err(BackendError::NoSuchWindow)
        | Err(BackendError::Refused(ActRefusal::Occluded, _)) => Ok(None),
        Err(e) => Err(backend_error(e)),
    }
}
