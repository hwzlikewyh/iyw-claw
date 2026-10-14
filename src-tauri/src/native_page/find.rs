use tauri_runtime_wry::wry::WebView;

#[cfg(target_os = "windows")]
pub(crate) fn search(
    view: &WebView,
    query: &str,
    backwards: bool,
    repeat: bool,
) -> Result<(), String> {
    use tauri_runtime_wry::wry::WebViewExtWindows;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Environment15, ICoreWebView2_28,
    };
    use windows::core::Interface;
    let Ok(core) = view.webview().cast::<ICoreWebView2_28>() else {
        return fallback(view, query, backwards);
    };
    let finder = unsafe { core.Find() }.map_err(|error| error.to_string())?;
    if query.is_empty() {
        return unsafe { finder.Stop() }.map_err(|error| error.to_string());
    }
    if repeat {
        return unsafe {
            if backwards {
                finder.FindPrevious()
            } else {
                finder.FindNext()
            }
        }
        .map_err(|error| error.to_string());
    }
    let environment = view
        .environment()
        .cast::<ICoreWebView2Environment15>()
        .map_err(|error| error.to_string())?;
    let options = unsafe { environment.CreateFindOptions() }.map_err(|error| error.to_string())?;
    start_find(finder, options, query)
}

#[cfg(target_os = "windows")]
fn start_find(
    finder: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Find,
    options: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2FindOptions,
    query: &str,
) -> Result<(), String> {
    use webview2_com::FindStartCompletedHandler;
    use windows::core::HSTRING;
    unsafe {
        options
            .SetFindTerm(&HSTRING::from(query))
            .map_err(|error| error.to_string())?;
        options
            .SetIsCaseSensitive(false)
            .map_err(|error| error.to_string())?;
        options
            .SetShouldHighlightAllMatches(true)
            .map_err(|error| error.to_string())?;
        options
            .SetSuppressDefaultFindDialog(true)
            .map_err(|error| error.to_string())?;
        finder.Start(
            &options,
            &FindStartCompletedHandler::create(Box::new(|_| Ok(()))),
        )
    }
    .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn search(
    view: &WebView,
    query: &str,
    backwards: bool,
    _repeat: bool,
) -> Result<(), String> {
    fallback(view, query, backwards)
}

fn fallback(view: &WebView, query: &str, backwards: bool) -> Result<(), String> {
    view.evaluate_script(&format!(
        "window.__iywPageFind?.({}, {backwards})",
        serde_json::json!(query)
    ))
    .map_err(|error| error.to_string())
}
