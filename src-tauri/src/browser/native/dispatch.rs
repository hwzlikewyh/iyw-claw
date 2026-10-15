use std::sync::{Arc, Mutex};

#[cfg(not(target_os = "windows"))]
use serde_json::json;
use serde_json::Value;
use tauri::AppHandle;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

type Reply = oneshot::Sender<Result<Value, String>>;

pub(super) fn execute(
    app: AppHandle,
    request: Value,
    cancellation: CancellationToken,
    sender: Reply,
) {
    let id = request["id"].as_str().unwrap_or("");
    let action = request["action"].as_str().unwrap_or("");
    let values = &request["values"];
    match action {
        "create" => {
            let _ = sender.send(super::build::create(&app, id, values));
        }
        "close" => {
            super::downloads::close_tab(&app, id);
            let _ = sender.send(super::state::remove(id).map(|_| Value::Null));
        }
        "export_cookies" => export_cookies(id, sender),
        #[cfg(target_os = "windows")]
        "apply_cookies" => apply_cookies(id, values, sender),
        #[cfg(target_os = "windows")]
        "cookies" if values["args"][1].as_str().unwrap_or("get") == "get" => {
            windows_cookies(id, sender)
        }
        "close_all" => {
            super::downloads::clear(&app);
            super::state::clear();
            let _ = sender.send(Ok(Value::Null));
        }
        "cancel_download" => {
            let _ = sender.send(
                super::download_control::cancel(values["downloadId"].as_str().unwrap_or(""))
                    .map(|_| Value::Null),
            );
        }
        "eval" => evaluate(
            id,
            values["expression"].as_str().unwrap_or("null"),
            cancellation,
            sender,
        ),
        "capture" => capture(id, sender),
        "pointer" => pointer(id, values, cancellation, sender),
        "view" => {
            let _ = sender.send(super::window::apply(&app, id, values).map(|_| Value::Null));
        }
        _ => {
            let _ = sender.send(super::actions::operate(id, action, values));
        }
    }
}

fn export_cookies(id: &str, sender: Reply) {
    #[cfg(target_os = "windows")]
    windows_cookies(id, sender);
    #[cfg(not(target_os = "windows"))]
    {
        let cookies = super::state::view(id)
            .ok()
            .and_then(|view| view.cookies().ok())
            .map(|cookies| {
                cookies
                    .iter()
                    .map(super::cookies::to_value)
                    .collect::<Vec<_>>()
            });
        let _ = sender.send(Ok(json!({"cookies":cookies})));
    }
}

#[cfg(target_os = "windows")]
fn apply_cookies(id: &str, values: &Value, sender: Reply) {
    let result = super::state::view(id)
        .and_then(|view| super::windows_cookies::replace(&view, values, &values["previous"]));
    let _ = sender.send(result);
}

#[cfg(target_os = "windows")]
fn windows_cookies(id: &str, sender: Reply) {
    let reply = Arc::new(Mutex::new(Some(sender)));
    let shared = reply.clone();
    let result = super::state::view(id).and_then(|view| {
        super::windows_cookies::get(&view, move |result| {
            finish(&shared, result);
        })
    });
    if let Err(error) = result {
        finish(&reply, Err(error));
    }
}

fn pointer(id: &str, values: &Value, cancellation: CancellationToken, sender: Reply) {
    let reply = Arc::new(Mutex::new(Some(sender)));
    let shared = reply.clone();
    let result = super::pointer::start(
        id,
        values,
        cancellation,
        Box::new(move |result| finish(&shared, result)),
    );
    if let Err(error) = result {
        finish(&reply, Err(error));
    }
}

fn evaluate(id: &str, expression: &str, cancellation: CancellationToken, sender: Reply) {
    let reply = Arc::new(Mutex::new(Some(sender)));
    let shared = reply.clone();
    let result = super::state::view(id).and_then(|view| {
        super::eval::start(
            &view,
            expression,
            cancellation,
            Box::new(move |result| finish(&shared, result)),
        )
    });
    if let Err(error) = result {
        finish(&reply, Err(error));
    }
}

fn capture(id: &str, sender: Reply) {
    let reply = Arc::new(Mutex::new(Some(sender)));
    let shared = reply.clone();
    let result = super::state::view(id).and_then(|view| {
        crate::native_page::capture::start(&view, Box::new(move |result| finish(&shared, result)))
    });
    if let Err(error) = result {
        finish(&reply, Err(error));
    }
}

fn finish(reply: &Arc<Mutex<Option<Reply>>>, result: Result<Value, String>) {
    if let Some(sender) = reply.lock().ok().and_then(|mut value| value.take()) {
        let _ = sender.send(result);
    }
}
