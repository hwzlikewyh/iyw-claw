use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use tauri_runtime_wry::wry::WebViewBuilder;

use super::super::cdp_records::DownloadRecord;
use super::super::{BrowserDownloadStatus, BrowserSessionManager};

thread_local! {
    static DOWNLOADS: RefCell<HashMap<PathBuf, DownloadRecord>> = RefCell::new(HashMap::new());
    static SOURCES: RefCell<HashMap<PathBuf, String>> = RefCell::new(HashMap::new());
}

pub(super) fn attach<'a>(
    builder: WebViewBuilder<'a>,
    app: &AppHandle,
    id: &str,
) -> WebViewBuilder<'a> {
    let start_app = app.clone();
    let end_app = app.clone();
    let tab_id = id.to_string();
    builder
        .with_download_started_handler(move |url, destination| {
            match prepare(&start_app, (&tab_id, &url), destination) {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(error, "native browser download could not be prepared");
                    false
                }
            }
        })
        .with_download_completed_handler(move |url, path, success| {
            if let Some(path) = path {
                finish_path(&end_app, path, success);
            } else {
                finish_without_path(&end_app, &url);
            }
        })
}

fn prepare(app: &AppHandle, source: (&str, &str), destination: &mut PathBuf) -> Result<(), String> {
    let root = dirs::download_dir()
        .unwrap_or_else(|| crate::paths::iyw_claw_user_dir().join("browser/downloads"))
        .join("原助理");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let filename = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("download")
        .to_string();
    let id = uuid::Uuid::new_v4().to_string();
    let path = unique_path(&root, &filename, &id);
    *destination = path.clone();
    let record = DownloadRecord {
        id: id.clone(),
        tab_id: Some(source.0.to_string()),
        suggested_filename: filename,
        status: BrowserDownloadStatus::InProgress,
        received_bytes: 0,
        total_bytes: None,
        completed_path: None,
    };
    let manager = app.state::<BrowserSessionManager>().inner().clone();
    SOURCES.with(|sources| {
        sources
            .borrow_mut()
            .insert(path.clone(), source.1.to_string())
    });
    DOWNLOADS.with(|downloads| downloads.borrow_mut().insert(path, record.clone()));
    tauri::async_runtime::spawn(async move {
        manager
            .state
            .write()
            .await
            .downloads
            .entry(id)
            .or_insert(record);
    });
    Ok(())
}

fn unique_path(root: &Path, filename: &str, id: &str) -> PathBuf {
    let requested = root.join(filename);
    if !requested.exists()
        && !DOWNLOADS.with(|downloads| downloads.borrow().contains_key(&requested))
    {
        return requested;
    }
    let name = Path::new(filename);
    let stem = name
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    let extension = name
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    root.join(format!("{stem}-{}{extension}", &id[..8]))
}

pub(super) fn record(path: &Path) -> Option<DownloadRecord> {
    DOWNLOADS.with(|downloads| downloads.borrow().get(path).cloned())
}

#[cfg(target_os = "windows")]
pub(super) fn progress(app: &AppHandle, path: &Path, received: u64, total: Option<u64>) {
    let record = DOWNLOADS.with(|downloads| {
        let mut downloads = downloads.borrow_mut();
        let record = downloads.get_mut(path)?;
        record.received_bytes = received;
        record.total_bytes = total;
        Some(record.clone())
    });
    if let Some(record) = record {
        publish(app, record);
    }
}

#[cfg(target_os = "windows")]
pub(super) fn mark_cancelled(id: &str) {
    DOWNLOADS.with(|downloads| {
        for record in downloads
            .borrow_mut()
            .values_mut()
            .filter(|record| record.id == id)
        {
            record.status = BrowserDownloadStatus::Cancelled;
        }
    });
}

pub(super) fn close_tab(app: &AppHandle, id: &str) {
    let paths = DOWNLOADS.with(|downloads| {
        downloads
            .borrow()
            .iter()
            .filter(|(_, record)| record.tab_id.as_deref() == Some(id))
            .map(|(path, record)| (path.clone(), record.id.clone()))
            .collect::<Vec<_>>()
    });
    for (path, id) in paths {
        super::download_control::forget(&id);
        finish_path(app, path, false);
    }
}

pub(super) fn clear(app: &AppHandle) {
    let paths = DOWNLOADS.with(|downloads| {
        downloads
            .borrow()
            .iter()
            .map(|(path, record)| (path.clone(), record.id.clone()))
            .collect::<Vec<_>>()
    });
    for (path, id) in paths {
        super::download_control::forget(&id);
        finish_path(app, path, false);
    }
}

pub(super) fn finish_path(app: &AppHandle, path: PathBuf, success: bool) {
    SOURCES.with(|sources| sources.borrow_mut().remove(&path));
    let Some(mut record) = DOWNLOADS.with(|downloads| downloads.borrow_mut().remove(&path)) else {
        return;
    };
    record.status = if record.status == BrowserDownloadStatus::Cancelled {
        BrowserDownloadStatus::Cancelled
    } else if success {
        BrowserDownloadStatus::Completed
    } else {
        BrowserDownloadStatus::Failed
    };
    if success {
        record.completed_path = Some(path.to_string_lossy().to_string());
        record.received_bytes = std::fs::metadata(&path)
            .map(|value| value.len())
            .unwrap_or(0);
    }
    publish(app, record);
}

fn finish_without_path(app: &AppHandle, url: &str) {
    // Wry 的失败回调只提供 URL；仅在身份唯一时更新，不能误标并发下载。
    let paths = SOURCES.with(|sources| {
        sources
            .borrow()
            .iter()
            .filter(|(_, source)| source.as_str() == url)
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>()
    });
    if paths.len() == 1 {
        finish_path(app, paths[0].clone(), false);
    } else if paths.len() > 1 {
        tracing::warn!(
            download_count = paths.len(),
            "native failed download identity is ambiguous"
        );
    }
}

fn publish(app: &AppHandle, mut record: DownloadRecord) {
    let manager = app.state::<BrowserSessionManager>().inner().clone();
    tauri::async_runtime::spawn(async move {
        let mut state = manager.state.write().await;
        if record.status == BrowserDownloadStatus::InProgress
            && state
                .downloads
                .get(&record.id)
                .is_some_and(|current| current.status != BrowserDownloadStatus::InProgress)
        {
            return;
        }
        if state
            .downloads
            .get(&record.id)
            .is_some_and(|current| current.status == BrowserDownloadStatus::Cancelled)
        {
            record.status = BrowserDownloadStatus::Cancelled;
        }
        let finished = record.status != BrowserDownloadStatus::InProgress;
        state.downloads.insert(record.id.clone(), record);
        drop(state);
        if finished {
            manager.spawn_runtime_idle_check("native_download_finished");
        }
    });
}
