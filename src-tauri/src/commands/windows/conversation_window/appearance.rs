use super::super::{
    apply_platform_window_style, post_window_setup, register_remote_window_cleanup,
};
use crate::app_error::AppCommandError;
use tauri::{AppHandle, WebviewWindow, WebviewWindowBuilder};

const WINDOW_WIDTH: f64 = 1120.0;
const WINDOW_HEIGHT: f64 = 760.0;
const MIN_WINDOW_WIDTH: f64 = 640.0;
const MIN_WINDOW_HEIGHT: f64 = 480.0;

pub(crate) fn build_window(
    app: &AppHandle,
    label: &str,
    route_context: (String, Option<&str>),
) -> Result<(), AppCommandError> {
    let (route, remote_id) = route_context;
    let builder = WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App(route.into()))
        .title("原助理")
        .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .min_inner_size(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)
        .visible(false)
        .center();
    let window = apply_platform_window_style(builder).build().map_err(|e| {
        AppCommandError::window("Could not create conversation window", e.to_string())
    })?;
    register_remote_window_cleanup(app, &window, remote_id);
    post_window_setup(&window);
    place_at_cursor(app, &window);
    Ok(())
}

fn place_at_cursor(app: &AppHandle, window: &WebviewWindow) {
    let Ok(cursor) = app.cursor_position() else {
        return;
    };
    let Ok(Some(monitor)) = app.monitor_from_point(cursor.x, cursor.y) else {
        return;
    };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let width = (WINDOW_WIDTH * scale).min(area.size.width as f64);
    let height = (WINDOW_HEIGHT * scale).min(area.size.height as f64);
    let x = cursor.x.clamp(
        area.position.x as f64,
        area.position.x as f64 + area.size.width as f64 - width,
    );
    let y = cursor.y.clamp(
        area.position.y as f64,
        area.position.y as f64 + area.size.height as f64 - height,
    );
    if let Err(error) = window.set_position(tauri::PhysicalPosition::new(x, y)) {
        tracing::warn!(window_label = window.label(), %error, "[conversation-window] placement failed");
    }
}
