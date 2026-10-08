use tauri::WebviewWindow;

pub(crate) fn install_main_microphone_permission(window: &WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        return windows::install(window);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use reqwest::Url;
    use tauri::webview::PlatformWebview;
    use tauri::WebviewWindow;
    use webview2_com::{
        take_pwstr, Microsoft::Web::WebView2::Win32::*, PermissionRequestedEventHandler,
    };
    use windows_core::PWSTR;

    const APP_HOSTS: [&str; 3] = ["tauri.localhost", "localhost", "127.0.0.1"];

    pub(super) fn install(window: &WebviewWindow) -> Result<(), String> {
        window
            .with_webview(|platform| {
                if let Err(error) = install_on_webview(&platform) {
                    tracing::warn!(
                        error = %error,
                        "main WebView microphone permission handler was not installed"
                    );
                }
            })
            .map_err(|error| error.to_string())
    }

    fn install_on_webview(platform: &PlatformWebview) -> Result<(), String> {
        let webview =
            unsafe { platform.controller().CoreWebView2() }.map_err(|error| error.to_string())?;
        let handler = PermissionRequestedEventHandler::create(Box::new(|_, args| {
            let Some(args) = args else {
                return Ok(());
            };

            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            unsafe { args.PermissionKind(&mut kind)? };
            if kind != COREWEBVIEW2_PERMISSION_KIND_MICROPHONE {
                return Ok(());
            }

            let mut uri = PWSTR::null();
            unsafe { args.Uri(&mut uri)? };
            if !is_app_origin(&take_pwstr(uri)) {
                return Ok(());
            }

            unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)? };
            Ok(())
        }));
        let mut token = 0_i64;
        unsafe { webview.add_PermissionRequested(&handler, &mut token) }
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn is_app_origin(uri: &str) -> bool {
        let Ok(url) = Url::parse(uri) else {
            return false;
        };
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some_and(|host| APP_HOSTS.contains(&host))
    }
}
