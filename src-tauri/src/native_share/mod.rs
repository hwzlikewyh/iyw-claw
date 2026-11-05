use iyw_share_core::{Inbox, Result};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

mod registration;
pub use registration::{register, status, ShareStatus};

#[cfg(target_os = "macos")]
#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShareConfig {
    app_group: Option<String>,
}

pub fn inbox(app: &AppHandle) -> Result<Inbox> {
    #[cfg(target_os = "macos")]
    {
        let path = resources(app)?.join("config.json");
        if path.exists() {
            let config: ShareConfig = serde_json::from_slice(&std::fs::read(path)?)?;
            if let Some(group) = config.app_group.filter(|value| !value.is_empty()) {
                return Ok(Inbox::new(mac_group_root(&group)?));
            }
        }
    }
    let _ = app;
    Ok(Inbox::new(iyw_share_core::default_root()?))
}

pub fn resources(app: &AppHandle) -> Result<PathBuf> {
    app.path()
        .resource_dir()
        .map(|root| root.join("native-share"))
        .map_err(|_| iyw_share_core::invalid("无法定位系统分享组件"))
}

#[cfg(target_os = "macos")]
fn mac_group_root(group: &str) -> Result<PathBuf> {
    use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
    use objc2_foundation::NSString;
    if !group.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
        return Err(iyw_share_core::invalid("共享容器标识无效"));
    }
    unsafe {
        let manager: Retained<AnyObject> = msg_send![class!(NSFileManager), defaultManager];
        let identifier = NSString::from_str(group);
        let url: Option<Retained<AnyObject>> = msg_send![&*manager,
            containerURLForSecurityApplicationGroupIdentifier: &*identifier];
        let url = url.ok_or_else(|| iyw_share_core::invalid("无法访问系统分享共享容器"))?;
        let path: Retained<NSString> = msg_send![&*url, path];
        Ok(PathBuf::from(path.to_string()).join("share-inbox"))
    }
}

struct InboxWatcher {
    _watcher: std::sync::Mutex<notify::RecommendedWatcher>,
}

pub fn setup(app: &AppHandle) {
    if let Err(error) = watch(app) {
        tracing::warn!(target: "iyw_native_share", error = %error, "share watcher unavailable; inbox remains readable");
    }
}

fn watch(app: &AppHandle) -> Result<()> {
    use notify::Watcher;
    let directory = inbox(app)?.root.join("Ready");
    std::fs::create_dir_all(&directory)?;
    let handle = app.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        match result {
            Ok(event) if matches!(event.kind, notify::EventKind::Create(_) | notify::EventKind::Modify(_)) => {
                let _ = handle.emit_to("main", "native-share://changed", ());
                crate::commands::windows::show_main_window(&handle);
            }
            Err(error) => tracing::warn!(target: "iyw_native_share", error = %error, "share directory notification failed"),
            _ => {}
        }
    }).map_err(|error| iyw_share_core::invalid(&error.to_string()))?;
    watcher
        .watch(&directory, notify::RecursiveMode::NonRecursive)
        .map_err(|error| iyw_share_core::invalid(&error.to_string()))?;
    app.manage(InboxWatcher {
        _watcher: std::sync::Mutex::new(watcher),
    });
    Ok(())
}
