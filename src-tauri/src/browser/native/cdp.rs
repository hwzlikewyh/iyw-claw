#[cfg(target_os = "windows")]
use serde_json::Value;

#[cfg(target_os = "windows")]
pub(super) fn call(
    core: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
    method: &str,
    params: Value,
    done: impl FnOnce(Result<Value, String>) + 'static,
) -> Result<(), String> {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::HSTRING;
    let name = HSTRING::from(method);
    let params = HSTRING::from(params.to_string());
    let method = method.to_string();
    let handler =
        CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, json| {
            let value = result
                .map_err(|error| format!("{method}: {error}"))
                .and_then(|_| serde_json::from_str(&json).map_err(|error| error.to_string()));
            done(value);
            Ok(())
        }));
    // SAFETY: 只在主线程调用，WebView2 持有回调直至操作完成。
    unsafe { core.CallDevToolsProtocolMethod(&name, &params, &handler) }
        .map_err(|error| error.to_string())
}
