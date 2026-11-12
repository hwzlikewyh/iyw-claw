#[cfg(target_os = "windows")]
use super::resources;
use iyw_share_core::{invalid, Result};
use tauri::AppHandle;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareStatus {
    pub platform: &'static str,
    pub installed: bool,
    pub can_register: bool,
}

pub fn status(app: &AppHandle) -> Result<ShareStatus> {
    #[cfg(target_os = "windows")]
    {
        let root = resources(app)?;
        let signed = std::fs::read(root.join("config.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value.get("signed").and_then(|value| value.as_bool()))
            .unwrap_or(false);
        return Ok(ShareStatus {
            platform: "windows",
            installed: signed && super::registration_windows::installed(&root),
            can_register: signed && root.join("IywAssistant.ShareTarget.msix").is_file(),
        });
    }
    #[cfg(target_os = "macos")]
    {
        let executable = std::env::current_exe()?;
        let installed = executable
            .parent()
            .and_then(|path| path.parent())
            .map(|path| path.join("PlugIns/IywAssistantShare.appex").is_dir())
            .unwrap_or(false);
        return Ok(ShareStatus {
            platform: "macos",
            installed,
            can_register: false,
        });
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = app;
        Ok(ShareStatus {
            platform: "other",
            installed: false,
            can_register: false,
        })
    }
}

pub async fn register(app: &AppHandle) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        super::registration_windows::register(&resources(app)?).await
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Err(invalid("请在系统设置的共享扩展中启用原助理"))
    }
}

pub async fn register_if_missing(app: &AppHandle) -> Result<()> {
    let handle = app.clone();
    let required = tokio::task::spawn_blocking(move || -> Result<bool> {
        let state = status(&handle)?;
        Ok(state.can_register && !state.installed)
    })
    .await
    .map_err(|_| invalid("分享入口状态检查未完成"))??;
    if required {
        tracing::info!(target: "iyw_native_share", "registering native share for current user");
        register(app).await?;
        tracing::info!(target: "iyw_native_share", "native share registration verified");
    }
    Ok(())
}
