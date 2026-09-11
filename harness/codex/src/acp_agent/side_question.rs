//! A side request has its own responder, cancellation and notification lane.
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use codex_app_server_client::InProcessAppServerRequestHandle;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, watch};

use crate::UpstreamEvent;

#[path = "side_question_request.rs"]
mod request;
pub(super) use request::SideInput;

struct ActiveSide {
    id: String,
    cancel: watch::Sender<bool>,
    thread: Arc<Mutex<Option<String>>>,
    events: mpsc::Sender<Value>,
}

#[derive(Default)]
pub(super) struct SideQuestions {
    active: Option<ActiveSide>,
    seen: VecDeque<String>,
    quarantined: Arc<AtomicBool>,
}

impl SideQuestions {
    pub fn dispatch(
        &mut self,
        handle: InProcessAppServerRequestHandle,
        input: SideInput,
        params: &Value,
        response: oneshot::Sender<Result<Value, String>>,
    ) {
        if params["sessionId"].as_str() != Some(input.parent.as_str()) {
            let _ = response.send(Err(
                "Side-question session is not owned by this bridge".into()
            ));
            return;
        }
        let action = params["action"].as_str().unwrap_or_default();
        if action == "capabilities" {
            let _ = response.send(Ok(json!({"supported": true, "mode": "native-read-only"})));
            return;
        }
        let id = params["requestId"].as_str().unwrap_or_default().to_string();
        if id.is_empty()
            || id.len() > 100
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            let _ = response.send(Err("Invalid side-question request id".into()));
            return;
        }
        if action == "cancel" {
            if let Some(active) = self.active.as_ref().filter(|active| active.id == id) {
                let _ = active.cancel.send(true);
            }
            self.remember(id);
            let _ = response.send(Ok(json!({"cancelled": true})));
            return;
        }
        if action != "ask" || input.question.trim().is_empty() || input.question.len() > 32_768 {
            let _ = response.send(Err("Invalid side question".into()));
            return;
        }
        if self.quarantined.load(Ordering::Acquire) {
            let _ = response.send(Err(
                "Previous native side request has unconfirmed cleanup; reconnect before retrying"
                    .into(),
            ));
            return;
        }
        if self.seen.contains(&id) {
            let _ = response.send(Err("Side-question request already used or cancelled".into()));
            return;
        }
        // The sender remains until the previous collector and cleanup exit.
        if self
            .active
            .as_ref()
            .is_some_and(|active| !active.events.is_closed())
        {
            let _ = response.send(Err("A side question is already running".into()));
            return;
        }
        self.remember(id.clone());
        let (cancel, cancellation) = watch::channel(false);
        let (events, receiver) = mpsc::channel(128);
        let thread = Arc::new(Mutex::new(None));
        self.active = Some(ActiveSide {
            id: id.clone(),
            cancel,
            thread: thread.clone(),
            events,
        });
        let quarantined = self.quarantined.clone();
        tokio::spawn(async move {
            eprintln!("[side-question] request={id} state=started");
            let question = input.question.clone();
            let result =
                request::run(handle, input, thread, receiver, cancellation, quarantined).await;
            eprintln!(
                "[side-question] request={id} state={}",
                if result.is_ok() { "settled" } else { "failed" }
            );
            if let Err(error) = &result {
                eprintln!(
                    "[side-question] request={id} detail={}",
                    crate::diagnostics::safe_detail(
                        &error.replace(&question, "[side question redacted]")
                    )
                );
            }
            let _ = response.send(result);
        });
    }

    pub fn route(&self, event: &UpstreamEvent) -> bool {
        let Some(active) = &self.active else {
            return false;
        };
        let UpstreamEvent::ServerNotification { method, params } = event else {
            return false;
        };
        let hidden = active.thread.lock().unwrap_or_else(|e| e.into_inner());
        let id = params["threadId"]
            .as_str()
            .or_else(|| params.pointer("/thread/id").and_then(Value::as_str));
        if hidden.is_none() || id != hidden.as_deref() {
            return false;
        }
        if method == "item/completed" || method == "turn/completed" {
            if active
                .events
                .try_send(json!({"method": method, "params": params}))
                .is_err()
            {
                let _ = active.cancel.send(true);
            }
        }
        true
    }

    fn remember(&mut self, id: String) {
        if !self.seen.contains(&id) {
            self.seen.push_back(id);
        }
        while self.seen.len() > 256 {
            self.seen.pop_front();
        }
    }
}

impl Drop for SideQuestions {
    fn drop(&mut self) {
        // Do not abort the task: it must interrupt/unsubscribe its own thread.
        if let Some(active) = &self.active {
            let _ = active.cancel.send(true);
        }
    }
}
