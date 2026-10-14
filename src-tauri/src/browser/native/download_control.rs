#[cfg(target_os = "windows")]
use std::cell::RefCell;
#[cfg(target_os = "windows")]
use std::collections::HashMap;

use tauri::AppHandle;
use tauri_runtime_wry::wry::WebView;

#[cfg(target_os = "windows")]
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2DownloadOperation;

#[cfg(target_os = "windows")]
thread_local! {
    static OPERATIONS: RefCell<HashMap<String, ICoreWebView2DownloadOperation>> = RefCell::new(HashMap::new());
}

#[cfg(target_os = "windows")]
pub(super) fn install(view: &WebView, app: &AppHandle, tab_id: &str) -> Result<(), String> {
    use tauri_runtime_wry::wry::WebViewExtWindows;
    use webview2_com::{
        take_pwstr, DownloadStartingEventHandler, Microsoft::Web::WebView2::Win32::*,
    };
    use windows::core::{Interface, PWSTR};
    let core = view
        .webview()
        .cast::<ICoreWebView2_4>()
        .map_err(|error| error.to_string())?;
    let app = app.clone();
    let tab_id = tab_id.to_string();
    let handler = DownloadStartingEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let operation = unsafe { args.DownloadOperation()? };
        let mut path = PWSTR::null();
        unsafe {
            args.ResultFilePath(&mut path)?;
        }
        let path = std::path::PathBuf::from(take_pwstr(path));
        let Some(record) = super::downloads::record(&path) else {
            return Ok(());
        };
        attach_operation(&operation, &app, path)?;
        tracing::info!(
            browser_tab_id = tab_id,
            download_id = record.id,
            "native download control attached"
        );
        Ok(())
    }));
    let mut token = 0;
    unsafe { core.add_DownloadStarting(&handler, &mut token) }.map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn attach_operation(
    operation: &ICoreWebView2DownloadOperation,
    app: &AppHandle,
    path: std::path::PathBuf,
) -> windows::core::Result<()> {
    use webview2_com::{Microsoft::Web::WebView2::Win32::*, StateChangedEventHandler};
    let Some(record) = super::downloads::record(&path) else {
        return Ok(());
    };
    let id = record.id;
    OPERATIONS.with(|operations| {
        operations
            .borrow_mut()
            .insert(id.clone(), operation.clone())
    });
    let next_app = app.clone();
    let next_path = path.clone();
    let done = StateChangedEventHandler::create(Box::new(move |operation, _| {
        let Some(operation) = operation else {
            return Ok(());
        };
        let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
        unsafe {
            operation.State(&mut state)?;
        }
        if state != COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS {
            OPERATIONS.with(|operations| operations.borrow_mut().remove(&id));
            super::downloads::finish_path(
                &next_app,
                next_path.clone(),
                state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED,
            );
        }
        Ok(())
    }));
    let mut token = 0;
    unsafe {
        operation.add_StateChanged(&done, &mut token)?;
    }
    attach_progress(operation, app, path)
}

#[cfg(target_os = "windows")]
fn attach_progress(
    operation: &ICoreWebView2DownloadOperation,
    app: &AppHandle,
    path: std::path::PathBuf,
) -> windows::core::Result<()> {
    use webview2_com::BytesReceivedChangedEventHandler;
    let app = app.clone();
    let progress = BytesReceivedChangedEventHandler::create(Box::new(move |operation, _| {
        if let Some(operation) = operation {
            let (mut received, mut total) = (0_i64, 0_i64);
            unsafe {
                operation.BytesReceived(&mut received)?;
                operation.TotalBytesToReceive(&mut total)?;
            }
            super::downloads::progress(
                &app,
                &path,
                received.max(0) as u64,
                (total > 0).then_some(total as u64),
            );
        }
        Ok(())
    }));
    let mut token = 0;
    unsafe { operation.add_BytesReceivedChanged(&progress, &mut token) }
}

#[cfg(target_os = "windows")]
pub(super) fn cancel(id: &str) -> Result<(), String> {
    let operation = OPERATIONS
        .with(|operations| operations.borrow().get(id).cloned())
        .ok_or("Native download is no longer active")?;
    unsafe { operation.Cancel() }.map_err(|error| error.to_string())?;
    super::downloads::mark_cancelled(id);
    Ok(())
}

#[cfg(target_os = "windows")]
pub(super) fn forget(id: &str) {
    let operation = OPERATIONS.with(|operations| operations.borrow_mut().remove(id));
    if let Some(operation) = operation {
        let _ = unsafe { operation.Cancel() };
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn forget(_id: &str) {}

#[cfg(not(target_os = "windows"))]
pub(super) fn install(_view: &WebView, _app: &AppHandle, _tab_id: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub(super) fn cancel(_id: &str) -> Result<(), String> {
    Err("Cancel this download in the browser's native download interface".into())
}
