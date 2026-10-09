// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

// The panel's commands. Each is a `_core` function the desktop app's Tauri
// command and iyw-claw-server's HTTP handler (`web::handlers::computer`) both
// call; what only one of them can do — open System Settings, show a file in
// the Finder, size the strip — is the command's own.

pub async fn computer_status_core(
    service: &ComputerService,
) -> Result<ComputerStatus, AppCommandError> {
    let config = service.config.snapshot().await;
    // Asking for the helper's permissions starts the helper, which fetches
    // the driver on first use; only worth doing once the person has switched
    // computer use on.
    let permissions = if config.enabled {
        service.backend.permissions().await.ok()
    } else {
        None
    };
    Ok(ComputerStatus {
        enabled: config.enabled,
        platform: platform_name(),
        verified_platform: false,
        backend: service.backend.status().await,
        permissions,
        host: host_tcc(),
        shared: service.targets.shared(),
        screen_offered: cfg!(any(target_os = "macos", windows)) && config.screen_enabled,
    })
}
