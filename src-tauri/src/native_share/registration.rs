#[cfg(target_os = "windows")]
use super::resources;
use iyw_share_core::{invalid, Result};
use tauri::AppHandle;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(target_os = "windows")]
const REGISTER_TIMEOUT_SECS: u64 = 60;

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
        use std::os::windows::process::CommandExt;
        let root = resources(app)?;
        let helper = root.join("iyw-share-target.exe");
        let installed = helper.is_file()
            && std::process::Command::new(helper)
                .arg("--registration-check")
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
        let signed = std::fs::read(root.join("config.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value.get("signed").and_then(|value| value.as_bool()))
            .unwrap_or(false);
        return Ok(ShareStatus {
            platform: "windows",
            installed,
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
        let script = resources(app)?.join("register.ps1");
        if !script.is_file() {
            return Err(invalid("当前安装包没有系统分享注册组件"));
        }
        let mut command = tokio::process::Command::new("powershell.exe");
        command
            .env_remove("PSModulePath")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(script)
            .creation_flags(CREATE_NO_WINDOW)
            .kill_on_drop(true);
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(REGISTER_TIMEOUT_SECS),
            command.output(),
        )
        .await
        .map_err(|_| invalid("系统分享入口注册超时"))??;
        if !output.status.success() {
            tracing::warn!(target: "iyw_native_share", code = ?output.status.code(), "share package registration failed");
            return Err(invalid(
                "系统分享入口注册失败，请检查签名信任、系统版本和应用包安装策略",
            ));
        }
        Ok(())
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
