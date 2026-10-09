// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Every window, for the share picker.
pub async fn computer_list_shareable_windows_core(
    service: &ComputerService,
) -> Result<Vec<PickerWindow>, AppCommandError> {
    let config = service.config.snapshot().await;
    if !config.enabled {
        return Ok(Vec::new());
    }
    let blocklist = blocklist_of(&config);
    let _turn = service.turn.lock().await;
    let windows = service
        .backend
        .list_windows(None)
        .await
        .map_err(backend_error)?;
    service.sweep().await;
    let (entries, ended) = service.targets.observe(&windows, None);
    service.announce(&ended);
    Ok(entries
        .into_iter()
        .filter(|e| e.worth_listing())
        .map(|e| {
            let whole_app = e.grant.as_ref().is_some_and(|g| g.scope == GrantScope::App);
            (e, whole_app)
        })
        .map(|(e, whole_app)| PickerWindow {
            whole_screen: e
                .grant
                .as_ref()
                .is_some_and(|g| g.scope == GrantScope::Screen),
            not_grantable: grantable(&e.app, &service.me, &blocklist).err(),
            level: e.grant.as_ref().map_or(GrantLevel::None, |g| g.level),
            app_id: whole_app
                .then(|| service.targets.app_share_of(&e.target_id))
                .flatten()
                .map(|share| share.app_id),
            whole_app,
            app_name: e.app.name.clone(),
            app_key: e.app.key().unwrap_or_default().to_string(),
            pid: e.app.pid,
            title: e.title,
            bounds: e.bounds,
            on_screen: e.on_screen,
            minimized: e.minimized.unwrap_or(false),
            hidden: e.hidden.unwrap_or(false),
            target_id: e.target_id,
        })
        .collect())
}
