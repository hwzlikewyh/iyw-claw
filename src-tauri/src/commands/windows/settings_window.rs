use super::{
    append_query_param, append_remote_context, apply_platform_window_style, post_window_setup,
    register_remote_window_cleanup, route_with_new_remote_window, SettingsWindowState,
};
use crate::app_error::AppCommandError;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const WINDOW_WIDTH: f64 = 1120.0;
const WINDOW_HEIGHT: f64 = 760.0;
const MIN_WINDOW_WIDTH: f64 = 720.0;
const MIN_WINDOW_HEIGHT: f64 = 540.0;

pub(super) struct SettingsWindowRequest {
    pub section: Option<String>,
    pub agent_type: Option<String>,
    pub remote_connection_id: Option<i32>,
    pub title: &'static str,
}

fn settings_route(request: &SettingsWindowRequest) -> String {
    let section = match request.section.as_deref() {
        Some("mcp") => "connectors",
        Some(
            section @ ("appearance"
            | "general"
            | "usage"
            | "performance"
            | "user-memory"
            | "agents"
            | "connectors"
            | "experts"
            | "office-tools"
            | "internet-tools"
            | "codex-native"
            | "computer-use"
            | "quick-messages"
            | "shortcuts"
            | "version-control"
            | "chat-channels"
            | "system"
            | "runtime-environment"
            | "skills"
            | "model-providers"
            | "logs"),
        ) => section,
        _ => "appearance",
    };
    let route = format!("settings/{section}");
    match request.agent_type.as_deref() {
        Some(agent) if section == "agents" => append_query_param(route, "agent", agent),
        _ => route,
    }
}

pub(super) fn open(
    app: &AppHandle,
    owner: &WebviewWindow,
    request: SettingsWindowRequest,
) -> Result<(), AppCommandError> {
    let label = match request.remote_connection_id {
        Some(id) => format!("remote-settings-{id}"),
        None => "settings".to_string(),
    };
    let route = settings_route(&request);
    tracing::info!(
        window_label = label,
        owner_label = owner.label(),
        section = route,
        "[settings-window] opening"
    );
    let state = app.state::<SettingsWindowState>();
    if let Some(existing) = app.get_webview_window(&label) {
        navigate_existing(&existing, route, request.remote_connection_id)?;
        state.set_owner(&label, owner.label());
        focus(&existing)?;
        tracing::info!(window_label = label, "[settings-window] reused");
        return Ok(());
    }

    let (route, remote_window_id) =
        route_with_new_remote_window(route, request.remote_connection_id);
    let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(route.into()))
        .title(request.title)
        .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .min_inner_size(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)
        .center();
    let window = apply_platform_window_style(builder)
        .build()
        .map_err(|e| AppCommandError::window("Failed to create settings window", e.to_string()))?;
    register_remote_window_cleanup(app, &window, remote_window_id.as_deref());
    post_window_setup(&window);
    state.set_owner(&label, owner.label());
    focus(&window)?;
    tracing::info!(window_label = label, "[settings-window] created");
    Ok(())
}

fn navigate_existing(
    window: &WebviewWindow,
    route: String,
    remote_connection_id: Option<i32>,
) -> Result<(), AppCommandError> {
    let current = window
        .url()
        .map_err(|e| AppCommandError::window("Failed to read settings URL", e.to_string()))?;
    let remote_window_id = current
        .query_pairs()
        .find(|(key, _)| key == "remoteWindowId")
        .map(|(_, value)| value.into_owned());
    let route = append_remote_context(route, remote_connection_id, remote_window_id.as_deref());
    let target = current
        .join(&format!("/{route}"))
        .map_err(|e| AppCommandError::window("Failed to resolve settings URL", e.to_string()))?;
    if current != target {
        window.navigate(target).map_err(|e| {
            AppCommandError::window("Failed to navigate settings window", e.to_string())
        })?;
    }
    Ok(())
}

fn focus(window: &WebviewWindow) -> Result<(), AppCommandError> {
    window
        .unminimize()
        .and_then(|_| window.show())
        .and_then(|_| window.set_focus())
        .map_err(|e| AppCommandError::window("Failed to focus settings window", e.to_string()))
}
