use serde_json::{json, Value};
use tauri::WebviewWindow;
use tokio::sync::oneshot;

use super::{build, capture, platform, state};
use state::PAGES;

type Reply = oneshot::Sender<Result<Value, String>>;

pub(super) fn dispatch(owner: WebviewWindow, request: Value, sender: Reply) {
    let Some(id) = request["id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 128)
    else {
        let _ = sender.send(Err("Native page id is required".into()));
        return;
    };
    let Some(action) = request["action"].as_str() else {
        let _ = sender.send(Err("Native page action is required".into()));
        return;
    };
    if action == "create" {
        let result = if PAGES.with(|pages| pages.borrow().contains_key(id)) {
            state::with_page(owner.label(), id, |page| Ok(state::snapshot(id, page)))
        } else {
            build::create(&owner, id, &request)
        };
        let _ = sender.send(result);
        return;
    }
    if action == "close" {
        let result = state::with_page(owner.label(), id, |_| Ok(()));
        if result.is_ok() {
            let removed = PAGES.with(|pages| pages.borrow_mut().remove(id));
            drop(removed);
            tracing::info!(page_id = id, owner = owner.label(), "native page closed");
        }
        let _ = sender.send(result.map(|_| Value::Null));
        return;
    }
    if action == "capture" {
        capture_reply(owner.label(), id, sender);
        return;
    }
    let result = operate(owner.label(), id, &request)
        .and_then(|_| state::with_page(owner.label(), id, |page| Ok(state::snapshot(id, page))));
    let _ = sender.send(result);
}

fn capture_reply(owner: &str, id: &str, sender: Reply) {
    let reply = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
    let done = reply.clone();
    let result = state::view(owner, id).and_then(|view| {
        capture::start(
            &view,
            Box::new(move |result| {
                if let Some(sender) = done.lock().ok().and_then(|mut sender| sender.take()) {
                    let _ = sender.send(result);
                }
            }),
        )
    });
    if let Err(error) = result {
        if let Some(sender) = reply.lock().ok().and_then(|mut sender| sender.take()) {
            let _ = sender.send(Err(error));
        }
    }
}

fn operate(owner: &str, id: &str, request: &Value) -> Result<(), String> {
    let action = request["action"].as_str().ok_or("Action is required")?;
    let view = state::view(owner, id)?;
    let result = match action {
        "bounds" => view.set_bounds(build::bounds(&request["bounds"])?),
        "visible" => view.set_visible(
            request["visible"]
                .as_bool()
                .ok_or("Visibility is required")?,
        ),
        "navigate" => view.load_url(&state::valid_url(
            request["url"].as_str().ok_or("URL is required")?,
        )?),
        "reload" => view.reload(),
        "back" | "forward" | "stop" => return platform::navigate(&view, action),
        "zoom" => {
            let zoom = request["zoom"]
                .as_f64()
                .filter(|value| value.is_finite() && (0.25..=5.0).contains(value))
                .ok_or("Invalid page zoom")?;
            view.zoom(zoom)
        }
        "devtools" => {
            view.open_devtools();
            return Ok(());
        }
        _ => return script_action(owner, id, request),
    };
    result.map_err(|error| error.to_string())
}

fn script_action(owner: &str, id: &str, request: &Value) -> Result<(), String> {
    let view = state::view(owner, id)?;
    let result = match request["action"].as_str().ok_or("Action is required")? {
        "pick" => {
            let token = uuid::Uuid::new_v4().to_string();
            state::with_page(owner, id, |page| {
                page.pick = Some(token.clone());
                Ok(())
            })?;
            view.evaluate_script(&format!("window.__iywPagePick({})", json!(token)))
        }
        "cancel-pick" => {
            state::with_page(owner, id, |page| {
                page.pick = None;
                Ok(())
            })?;
            view.evaluate_script("window.__iywPageCancel?.()")
        }
        "state" => return Ok(()),
        "find" => {
            let query = request["query"].as_str().ok_or("Find query is required")?;
            if query.len() > super::state::MAX_MESSAGE_BYTES {
                return Err("Find query exceeds the supported size".into());
            }
            let backwards = request["backwards"].as_bool().unwrap_or(false);
            let repeat = state::with_page(owner, id, |page| Ok(page.find_term == query))?;
            super::find::search(&view, query, backwards, repeat)?;
            state::with_page(owner, id, |page| {
                page.find_term = query.to_string();
                Ok(())
            })?;
            return Ok(());
        }
        _ => return Err("Unknown native page action".into()),
    };
    result.map_err(|error| error.to_string())
}
