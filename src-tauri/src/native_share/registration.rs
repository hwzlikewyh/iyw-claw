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
        let script = resources(app)?.join("register.ps1");
        if !script.is_file() {
            return Err(invalid("当前安装包没有系统分享注册组件"));
        }
        let mut command = tokio::process::Command::new("powershell.exe");
        command
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
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Err(invalid("请在系统设置的共享扩展中启用原助理"))
    }
}
