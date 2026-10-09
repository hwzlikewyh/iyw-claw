use std::sync::{Arc, OnceLock};

use crate::acp::computer_tools::ComputerToolsRuntimeConfig;
use crate::commands::computer::{ComputerHost, ComputerService};

static SERVICE: OnceLock<Arc<ComputerService>> = OnceLock::new();

pub fn current_service() -> Option<Arc<ComputerService>> {
    SERVICE.get().cloned()
}

pub fn runtime_supported() -> bool {
    if std::env::var("IYW_CLAW_RUNTIME").as_deref() == Ok("docker") {
        return false;
    }
    if super::driver::artifact_for_current_platform().is_none() {
        return false;
    }
    #[cfg(target_os = "linux")]
    return std::env::var_os("DISPLAY").is_some()
        && std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "x11");
    #[cfg(target_os = "macos")]
    return super::launch_req::supported()
        && (cfg!(feature = "tauri-runtime")
            || cfg!(debug_assertions)
            || super::local::HELPER_REQUIREMENT.is_some());
    #[cfg(windows)]
    return sysinfo::System::kernel_version()
        .and_then(|version| version.split('.').next()?.parse::<u32>().ok())
        .is_some_and(|major| major >= 10);
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    false
}

pub fn tools_enabled() -> bool {
    current_service().is_some_and(|service| service.tools_enabled())
}

#[derive(serde::Serialize)]
pub struct ComputerAvailability {
    pub available: bool,
    pub platform: &'static str,
    pub reason: Option<&'static str>,
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub fn computer_available() -> ComputerAvailability {
    let available = current_service().is_some();
    ComputerAvailability {
        available,
        platform: crate::commands::computer::platform_name(),
        reason: (!available)
            .then_some("Computer Use requires Windows 10+, macOS 14.4+, or an X11 desktop session"),
    }
}

#[cfg(feature = "tauri-runtime")]
pub fn start_desktop(app: &tauri::AppHandle, db: &sea_orm::DatabaseConnection) {
    use tauri::Manager;
    if let Err(error) =
        tauri::async_runtime::block_on(crate::commands::retired_computer_use::cleanup(db))
    {
        tracing::warn!(error = %error.message, "[computer] retired MCP cleanup failed; will retry next startup");
    }
    let config = ComputerToolsRuntimeConfig::new();
    tauri::async_runtime::block_on(
        crate::commands::computer_tools::apply_persisted_computer_tools_config(db, &config),
    );
    app.manage(config.clone());
    if runtime_supported() {
        let service = ComputerService::start(ComputerHost::Desktop(app.clone()), config);
        let _ = SERVICE.set(service.clone());
        app.manage(service);
    }
    tracing::info!(
        supported = runtime_supported(),
        "[computer] desktop runtime initialized"
    );
}

pub async fn start_server(state: &crate::app_state::AppState) {
    if let Err(error) = crate::commands::retired_computer_use::cleanup(&state.db.conn).await {
        tracing::warn!(error = %error.message, "[computer] retired MCP cleanup failed; will retry next startup");
    }
    crate::commands::computer_tools::apply_persisted_computer_tools_config(
        &state.db.conn,
        &state.computer_tools_config,
    )
    .await;
    if std::env::var("IYW_CLAW_COMPUTER_USE").as_deref() != Ok("1") || !runtime_supported() {
        return;
    }
    let service = ComputerService::start(
        ComputerHost::Server {
            broadcaster: state.event_broadcaster.clone(),
            emitter: state.emitter.clone(),
        },
        state.computer_tools_config.clone(),
    );
    let _ = SERVICE.set(service.clone());
    let _ = state.computer_service.set(service);
    tracing::info!("[computer] server runtime explicitly enabled");
}

pub async fn shutdown() {
    if let Some(service) = current_service() {
        service.shutdown().await;
    }
}

pub(crate) struct UpdatePause {
    service: Option<Arc<ComputerService>>,
    committed: bool,
}

impl UpdatePause {
    pub(crate) async fn begin() -> Self {
        let service = current_service();
        if let Some(service) = &service {
            service.shutdown().await;
        }
        Self {
            service,
            committed: false,
        }
    }

    pub(crate) fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for UpdatePause {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Some(service) = self.service.take() {
            tokio::spawn(async move {
                service.resume_after_failed_update().await;
            });
        }
    }
}
