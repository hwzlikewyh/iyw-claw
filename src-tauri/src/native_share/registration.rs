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
        let _ = app;
        return super::registration_macos::status();
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
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        tokio::task::spawn_blocking(super::registration_macos::register)
            .await
            .map_err(|_| invalid("分享扩展注册任务未完成"))?
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = app;
        Err(invalid("请在系统设置的共享扩展中启用原助理"))
    }
}

pub async fn register_if_missing(app: &AppHandle) -> Result<()> {
    let handle = app.clone();
    let required = tokio::task::spawn_blocking(move || -> Result<bool> {
        let state = status(&handle)?;
        if !state.can_register || state.installed {
            return Ok(false);
        }
        #[cfg(target_os = "macos")]
        if matches!(
            super::registration_macos::election()?,
            Some('-' | '!' | '?')
        ) {
            // 尊重用户禁用和系统策略；仅为新安装、从未选用的扩展初始化入口。
            return Ok(false);
        }
        Ok(true)
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
