use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::eval::Callback;
use tokio_util::sync::CancellationToken;

#[cfg(target_os = "windows")]
pub(super) fn start(
    id: &str,
    values: &Value,
    cancellation: CancellationToken,
    done: Callback,
) -> Result<(), String> {
    use tauri_runtime_wry::wry::WebViewExtWindows;
    let state = super::state::snapshot(id)?;
    let epoch = format!("{id}.{}", state["epoch"].as_u64().unwrap_or(0));
    if values["epoch"] != epoch {
        return Err("STALE_REF: the page moved before the pointer arrived".into());
    }
    let x = values["x"]
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or("Invalid pointer coordinate")?;
    let y = values["y"]
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or("Invalid pointer coordinate")?;
    super::state::with_page(id, |page| {
        page.agent_input_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(250));
        Ok(())
    })?;
    let core = super::state::view(id)?.webview();
    let events = if values["move"] == true {
        vec![json!({"type":"mouseMoved","x":x,"y":y,"pointerType":"mouse"})]
    } else {
        let count = values["count"].as_u64().unwrap_or(1).clamp(1, 2);
        (1..=count).flat_map(|click| [json!({"type":"mousePressed","x":x,"y":y,"button":"left","buttons":1,"clickCount":click}), json!({"type":"mouseReleased","x":x,"y":y,"button":"left","buttons":0,"clickCount":click})]).collect()
    };
    dispatch(core, events, cancellation, Arc::new(Mutex::new(Some(done))));
    Ok(())
}

#[cfg(target_os = "windows")]
fn dispatch(
    core: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
    mut events: Vec<Value>,
    cancellation: CancellationToken,
    reply: Arc<Mutex<Option<Callback>>>,
) {
    if events.is_empty() {
        finish(&reply, Ok(json!({"fidelity":"trusted"})));
        return;
    }
    let event = events.remove(0);
    if cancellation.is_cancelled() && event["type"] != "mouseReleased" {
        finish(&reply, Err("Browser operation cancelled".into()));
        return;
    }
    let next = core.clone();
    let shared = reply.clone();
    let result =
        super::cdp::call(
            &core,
            "Input.dispatchMouseEvent",
            event,
            move |result| match result {
                Ok(_) => dispatch(next, events, cancellation, shared),
                Err(error) => finish(&shared, Err(error)),
            },
        );
    if let Err(error) = result {
        finish(&reply, Err(error));
    }
}

#[cfg(target_os = "windows")]
fn finish(reply: &Arc<Mutex<Option<Callback>>>, result: Result<Value, String>) {
    if let Some(done) = reply.lock().ok().and_then(|mut reply| reply.take()) {
        done(result);
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn start(
    _id: &str,
    _values: &Value,
    _cancellation: CancellationToken,
    _done: Callback,
) -> Result<(), String> {
    Err("Trusted pointer input is unavailable on this platform".into())
}
