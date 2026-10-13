use tauri_runtime_wry::wry::WebView;

#[cfg(target_os = "windows")]
pub(super) fn history(view: &WebView) -> (bool, bool) {
    use tauri_runtime_wry::wry::WebViewExtWindows;
    use windows::core::BOOL;
    let core = view.webview();
    let mut back = BOOL::default();
    let mut forward = BOOL::default();
    unsafe {
        let _ = core.CanGoBack(&mut back);
        let _ = core.CanGoForward(&mut forward);
    }
    (back.as_bool(), forward.as_bool())
}

#[cfg(target_os = "windows")]
pub(super) fn navigate(view: &WebView, action: &str) -> Result<(), String> {
    use tauri_runtime_wry::wry::WebViewExtWindows;
    let core = view.webview();
    unsafe {
        match action {
            "back" => core.GoBack(),
            "forward" => core.GoForward(),
            _ => core.Stop(),
        }
    }
    .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
pub(super) fn history(view: &WebView) -> (bool, bool) {
    use tauri_runtime_wry::wry::WebViewExtMacOS;
    let wk = view.webview();
    unsafe { (wk.canGoBack(), wk.canGoForward()) }
}

#[cfg(target_os = "macos")]
pub(super) fn navigate(view: &WebView, action: &str) -> Result<(), String> {
    use tauri_runtime_wry::wry::WebViewExtMacOS;
    let wk = view.webview();
    unsafe {
        match action {
            "back" => {
                wk.goBack();
            }
            "forward" => {
                wk.goForward();
            }
            _ => wk.stopLoading(),
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub(super) fn history(view: &WebView) -> (bool, bool) {
    use tauri_runtime_wry::wry::WebViewExtUnix;
    use webkit2gtk::prelude::WebViewExt;
    let wk = view.webview();
    (wk.can_go_back(), wk.can_go_forward())
}

#[cfg(target_os = "linux")]
pub(super) fn navigate(view: &WebView, action: &str) -> Result<(), String> {
    use tauri_runtime_wry::wry::WebViewExtUnix;
    use webkit2gtk::prelude::WebViewExt;
    let wk = view.webview();
    match action {
        "back" => wk.go_back(),
        "forward" => wk.go_forward(),
        _ => wk.stop_loading(),
    }
    Ok(())
}
