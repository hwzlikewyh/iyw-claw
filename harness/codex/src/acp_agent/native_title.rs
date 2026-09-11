use std::sync::{Arc, Mutex};

use codex_app_server_client::InProcessAppServerRequestHandle;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::UpstreamEvent;

#[path = "native_title_request.rs"]
mod request;

const TITLE_EVENT_CAPACITY: usize = 32;
const TITLE_INPUT_CHARS: usize = 960;

#[derive(Default)]
pub(super) struct NativeTitle {
    attempted: bool,
    hidden_thread: Arc<Mutex<Option<String>>>,
    events: Option<mpsc::Sender<Value>>,
    task: Option<JoinHandle<Result<(), String>>>,
}

impl NativeTitle {
    pub(super) fn start(
        &mut self,
        handle: InProcessAppServerRequestHandle,
        input: request::TitleInput,
    ) {
        if self.attempted || input.prompt.trim().is_empty() {
            return;
        }
        self.attempted = true;
        let (events, receiver) = mpsc::channel(TITLE_EVENT_CAPACITY);
        self.events = Some(events);
        let hidden_thread = Arc::clone(&self.hidden_thread);
        self.task = Some(tokio::spawn(async move {
            request::generate(handle, input, (hidden_thread, receiver)).await
        }));
    }

    pub(super) fn route(&mut self, event: &UpstreamEvent) -> bool {
        let UpstreamEvent::ServerNotification { method, params } = event else {
            return false;
        };
        let id = self
            .hidden_thread
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if id.is_none() || params.get("threadId").and_then(Value::as_str) != id.as_deref() {
            return false;
        }
        if matches!(method.as_str(), "item/completed" | "turn/completed") {
            if let Some(sender) = &self.events {
                if sender
                    .try_send(json!({"method": method, "params": params}))
                    .is_err()
                {
                    self.events = None;
                }
            }
        }
        true
    }

    pub(super) async fn finished(&mut self) {
        let result = match self.task.as_mut() {
            Some(task) => task.await,
            None => return std::future::pending().await,
        };
        self.task = None;
        self.events = None;
        match result {
            Ok(Ok(())) => eprintln!("[internal-codex-worker] stage=native_title status=ok"),
            Ok(Err(error)) => eprintln!(
                "[internal-codex-worker] stage=native_title status=error detail={}",
                crate::diagnostics::safe_detail(&error)
            ),
            Err(_) => eprintln!("[internal-codex-worker] stage=native_title status=interrupted"),
        }
    }
}

impl Drop for NativeTitle {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

pub(super) use request::TitleInput;

pub(super) fn prompt_text(params: &Value) -> Option<String> {
    let text = params
        .get("prompt")?
        .as_array()?
        .iter()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .filter(|text| !text.starts_with("<!-- IYW_CLAW_USER_CONTEXT_V1_START -->"))
        .collect::<Vec<_>>()
        .join("\n");
    let text = text
        .trim()
        .chars()
        .take(TITLE_INPUT_CHARS)
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}
