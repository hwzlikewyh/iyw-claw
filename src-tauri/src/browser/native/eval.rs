use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tauri_runtime_wry::wry::WebView;
use tokio_util::sync::CancellationToken;

pub(super) type Callback = Box<dyn FnOnce(Result<Value, String>) + Send>;

pub(super) fn start(
    view: &WebView,
    expression: &str,
    cancellation: CancellationToken,
    done: Callback,
) -> Result<(), String> {
    let expression = format!("(async () => {{ try {{ return {{ok:true,value:await ({expression})}} }} catch(error) {{ return {{ok:false,error:String(error.message || error)}} }} }})()");
    evaluate(view, expression, cancellation, done)
}

fn decode(value: Value) -> Result<Value, String> {
    if value["ok"] == true {
        return Ok(value["value"].clone());
    }
    Err(value["error"]
        .as_str()
        .unwrap_or("Native page evaluation failed")
        .to_string())
}

#[cfg(target_os = "windows")]
fn evaluate(
    view: &WebView,
    expression: String,
    cancellation: CancellationToken,
    done: Callback,
) -> Result<(), String> {
    use tauri_runtime_wry::wry::WebViewExtWindows;
    let core = view.webview();
    let next = core.clone();
    let reply = Arc::new(Mutex::new(Some(done)));
    let shared = reply.clone();
    let result = super::cdp::call(&core, "Page.getFrameTree", json!({}), move |result| {
        let result = result.and_then(|value| {
            value["frameTree"]["frame"]["id"]
                .as_str()
                .map(str::to_string)
                .ok_or("Page frame is unavailable".into())
        });
        match result {
            Ok(frame) => create_world(next, frame, expression, cancellation, shared),
            Err(error) => finish(&shared, Err(error)),
        }
    });
    if let Err(error) = result {
        finish(&reply, Err(error));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn create_world(
    core: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
    frame: String,
    expression: String,
    cancellation: CancellationToken,
    reply: Arc<Mutex<Option<Callback>>>,
) {
    if cancellation.is_cancelled() {
        finish(&reply, Err("Browser operation cancelled".into()));
        return;
    }
    let next = core.clone();
    let shared = reply.clone();
    let result = super::cdp::call(
        &core,
        "Page.createIsolatedWorld",
        json!({"frameId": frame, "worldName": "iyw-browser-agent"}),
        move |result| match result.and_then(|value| {
            value["executionContextId"]
                .as_i64()
                .ok_or("Page world is unavailable".into())
        }) {
            Ok(context) => {
                if cancellation.is_cancelled() {
                    finish(&shared, Err("Browser operation cancelled".into()));
                } else {
                    evaluate_world(next, context, expression, shared);
                }
            }
            Err(error) => finish(&shared, Err(error)),
        },
    );
    if let Err(error) = result {
        finish(&reply, Err(error));
    }
}

#[cfg(target_os = "windows")]
fn evaluate_world(
    core: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
    context: i64,
    expression: String,
    reply: Arc<Mutex<Option<Callback>>>,
) {
    let shared = reply.clone();
    let result = super::cdp::call(
        &core,
        "Runtime.evaluate",
        json!({"expression": expression, "contextId": context, "returnByValue": true, "awaitPromise": true}),
        move |result| {
            finish(
                &shared,
                result.and_then(|value| {
                    if value.get("exceptionDetails").is_some() {
                        return Err("Native page evaluation was interrupted".into());
                    }
                    decode(value["result"]["value"].clone())
                }),
            );
        },
    );
    if let Err(error) = result {
        finish(&reply, Err(error));
    }
}

#[cfg(target_os = "macos")]
fn evaluate(
    view: &WebView,
    expression: String,
    cancellation: CancellationToken,
    done: Callback,
) -> Result<(), String> {
    if cancellation.is_cancelled() {
        return Err("Browser operation cancelled".into());
    }
    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSError, NSString};
    use objc2_web_kit::WKContentWorld;
    use tauri_runtime_wry::wry::WebViewExtMacOS;
    let mtm = objc2::MainThreadMarker::new().ok_or("Browser scripts need the main thread")?;
    let reply = Mutex::new(Some(done));
    let block = RcBlock::<dyn Fn(*mut AnyObject, *mut NSError)>::new(move |value, error| {
        let result = unsafe {
            if !error.is_null() {
                Err((*error).localizedDescription().to_string())
            } else if value.is_null() {
                Err("Browser script returned no result".into())
            } else {
                (*value)
                    .downcast_ref::<NSString>()
                    .ok_or("Browser script returned an invalid result".to_string())
                    .and_then(|value| {
                        serde_json::from_str(&value.to_string()).map_err(|error| error.to_string())
                    })
                    .and_then(decode)
            }
        };
        if let Some(done) = reply.lock().ok().and_then(|mut reply| reply.take()) {
            done(result);
        }
    });
    unsafe {
        let world = WKContentWorld::worldWithName(&NSString::from_str("iyw-browser-agent"), mtm);
        view.webview()
            .callAsyncJavaScript_arguments_inFrame_inContentWorld_completionHandler(
                &NSString::from_str(&format!("return JSON.stringify(await ({expression}))")),
                None,
                None,
                &world,
                Some(&block),
            );
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn evaluate(
    view: &WebView,
    expression: String,
    cancellation: CancellationToken,
    done: Callback,
) -> Result<(), String> {
    if cancellation.is_cancelled() {
        return Err("Browser operation cancelled".into());
    }
    use tauri_runtime_wry::wry::WebViewExtUnix;
    use webkit2gtk::prelude::WebViewExt;
    view.webview().call_async_javascript_function(
        &format!("return JSON.stringify(await ({expression}))"),
        None,
        Some("iyw-browser-agent"),
        None,
        None::<&webkit2gtk::gio::Cancellable>,
        move |result| {
            done(
                result
                    .map_err(|error| error.to_string())
                    .and_then(|value| {
                        serde_json::from_str(&value.to_string()).map_err(|error| error.to_string())
                    })
                    .and_then(decode),
            );
        },
    );
    Ok(())
}

fn finish(reply: &Arc<Mutex<Option<Callback>>>, result: Result<Value, String>) {
    if let Some(done) = reply.lock().ok().and_then(|mut value| value.take()) {
        done(result);
    }
}
