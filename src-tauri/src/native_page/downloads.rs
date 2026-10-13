use super::state::EVENT;
use serde_json::json;
use std::path::PathBuf;
use tauri::{Emitter, Manager, WebviewWindow};
use tauri_runtime_wry::wry::WebViewBuilder;

pub(super) fn attach<'a>(
    builder: WebViewBuilder<'a>,
    owner: &WebviewWindow,
    id: &str,
) -> WebViewBuilder<'a> {
    let started = owner.clone();
    let started_id = id.to_string();
    let finished = owner.clone();
    let finished_id = id.to_string();
    builder.with_download_started_handler(move |url, destination| {
        match download_path(&started, destination) {
            Ok(path) => {
                *destination = path;
                let _ = started.emit(EVENT, json!({"id": started_id, "type": "download", "url": url,
                    "name": destination.file_name().and_then(|name| name.to_str()), "status": "started"}));
                true
            }
            Err(error) => { tracing::warn!(error = %error, "native page download destination failed"); false }
        }
    }).with_download_completed_handler(move |url, path, success| {
        tracing::info!(page_id = finished_id, owner = finished.label(), success, "native page download finished");
        let _ = finished.emit(EVENT, json!({"id": finished_id, "type": "download", "url": url,
            "name": path.as_ref().and_then(|path| path.file_name()).and_then(|name| name.to_str()),
            "path": path, "status": if success { "completed" } else { "failed" }}));
    })
}

fn download_path(owner: &WebviewWindow, proposed: &std::path::Path) -> Result<PathBuf, String> {
    let directory = owner
        .app_handle()
        .path()
        .download_dir()
        .map_err(|error| error.to_string())?;
    let name = proposed
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    let directory = directory
        .join("interactive-pages")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join(name))
}
