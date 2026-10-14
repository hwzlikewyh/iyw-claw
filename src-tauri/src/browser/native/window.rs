use serde_json::Value;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewWindow};
use tauri_runtime_wry::wry::Rect;

pub(super) fn apply(app: &AppHandle, id: &str, request: &Value) -> Result<(), String> {
    let owner_name = request["owner"]
        .as_str()
        .ok_or("Browser host window is required")?;
    let owner = app
        .get_webview_window(owner_name)
        .ok_or("Browser host window is unavailable")?;
    let view = super::state::view(id)?;
    let previous = super::state::with_page(id, |page| Ok(page.owner.clone()))?;
    if previous != owner_name {
        #[cfg(target_os = "windows")]
        crate::native_page::window_hook::retain(&owner)?;
        view.set_visible(false).map_err(|error| error.to_string())?;
        reparent(&view, &owner)?;
        super::state::with_page(id, |page| {
            page.owner = owner_name.to_string();
            Ok(())
        })?;
    }
    #[cfg(not(target_os = "linux"))]
    if let Some(bounds) = request.get("bounds") {
        view.set_bounds(rect(bounds)?)
            .map_err(|error| error.to_string())?;
    }
    let visible = request["visible"]
        .as_bool()
        .ok_or("Browser visibility is required")?;
    view.set_visible(visible)
        .map_err(|error| error.to_string())?;
    #[cfg(target_os = "linux")]
    super::linux::visible(id, visible)?;
    super::state::with_page(id, |page| {
        page.visible = visible;
        Ok(())
    })?;
    Ok(())
}

fn rect(value: &Value) -> Result<Rect, String> {
    let number = |key: &str| {
        value[key]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("Invalid browser bounds: {key}"))
    };
    let x = number("x")?;
    let y = number("y")?;
    let width = number("width")?;
    let height = number("height")?;
    if width <= 0.0 || height <= 0.0 || width > 8192.0 || height > 8192.0 {
        return Err("Invalid browser viewport size".into());
    }
    Ok(Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(width, height).into(),
    })
}

#[cfg(target_os = "windows")]
fn reparent(view: &tauri_runtime_wry::wry::WebView, owner: &WebviewWindow) -> Result<(), String> {
    use tauri_runtime_wry::wry::WebViewExtWindows;
    view.reparent(owner.hwnd().map_err(|error| error.to_string())?.0 as isize)
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn reparent(view: &tauri_runtime_wry::wry::WebView, owner: &WebviewWindow) -> Result<(), String> {
    use tauri_runtime_wry::wry::WebViewExtMacOS;
    view.reparent(owner.ns_window().map_err(|error| error.to_string())?.cast())
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
fn reparent(_view: &tauri_runtime_wry::wry::WebView, _owner: &WebviewWindow) -> Result<(), String> {
    Ok(())
}
