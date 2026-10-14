//! Linux 与 codeg 一致使用独立窗口，避免 Wayland 子视图定位限制。
use std::cell::RefCell;
use std::collections::HashMap;

use tauri::{AppHandle, Manager, Window, WindowBuilder};
use tauri_runtime_wry::wry::{WebView, WebViewBuilder, WebViewBuilderExtUnix};

thread_local! {
    static WINDOWS: RefCell<HashMap<String, Window>> = RefCell::new(HashMap::new());
}

pub(super) fn build(
    app: &AppHandle,
    id: &str,
    builder: WebViewBuilder<'_>,
) -> Result<WebView, String> {
    let window = WindowBuilder::new(app, format!("native-page-{id}"))
        .title("Browser")
        .inner_size(1024.0, 768.0)
        .visible(false)
        .build()
        .map_err(|error| error.to_string())?;
    let gtk = window.gtk_window().map_err(|error| error.to_string())?;
    let view = match builder.build_gtk(&gtk) {
        Ok(view) => view,
        Err(error) => {
            let _ = window.destroy();
            return Err(error.to_string());
        }
    };
    let manager = app
        .state::<super::super::BrowserSessionManager>()
        .inner()
        .clone();
    let tab_id = id.to_string();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            let manager = manager.clone();
            let id = tab_id.clone();
            tauri::async_runtime::spawn(async move {
                let _ = manager.close_browser_tab(&id).await;
            });
        }
    });
    WINDOWS.with(|windows| windows.borrow_mut().insert(id.to_string(), window));
    Ok(view)
}

pub(super) fn visible(id: &str, visible: bool) -> Result<(), String> {
    let window = WINDOWS
        .with(|windows| windows.borrow().get(id).cloned())
        .ok_or("Browser window is closed")?;
    if visible {
        window.show()
    } else {
        window.hide()
    }
    .map_err(|error| error.to_string())
}

pub(super) fn remove(id: &str) {
    let window = WINDOWS.with(|windows| windows.borrow_mut().remove(id));
    if let Some(window) = window {
        let _ = window.destroy();
    }
}
