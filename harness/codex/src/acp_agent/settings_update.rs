use serde_json::Value;
use tokio::sync::oneshot;
use tokio::time::{Duration, Instant};

use super::settings_mapping::{self, SessionSettings};
use crate::{UpstreamClient, UpstreamError, UpstreamEvent};

// 宿主配置命令超时为 20 秒，预留回复和状态回滚时间。
const APPLY_TIMEOUT: Duration = Duration::from_secs(15);
type Reply = oneshot::Sender<Result<Value, String>>;

#[derive(Default)]
pub(super) struct SettingsUpdate {
    pending: Option<PendingUpdate>,
    deferred_prompt: Option<super::BridgeCommand>,
}

struct PendingUpdate {
    method: String,
    params: Value,
    response: Reply,
    deadline: Instant,
}

impl SettingsUpdate {
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|pending| pending.deadline)
    }

    pub(super) fn defer_prompt(&mut self, params: Value, responder: sacp::Responder<Value>) {
        if self.deferred_prompt.is_some() {
            let _ = responder.respond_with_error(super::to_sacp_error(
                "A prompt is already waiting for session settings",
            ));
        } else {
            self.deferred_prompt = Some(super::BridgeCommand::Prompt { params, responder });
        }
    }

    pub(super) fn take_prompt(&mut self) -> Option<super::BridgeCommand> {
        if self.pending.is_some() {
            None
        } else {
            self.deferred_prompt.take()
        }
    }

    pub(super) fn cancel_prompt(&mut self) {
        if let Some(super::BridgeCommand::Prompt { responder, .. }) = self.deferred_prompt.take() {
            let _ = responder.respond(serde_json::json!({ "stopReason": "cancelled" }));
        }
    }

    pub(super) async fn start(
        &mut self,
        context: (&UpstreamClient, &SessionSettings),
        input: (String, Value, Reply),
    ) {
        let (method, params, response) = input;
        if self.pending.is_some() {
            let _ = response.send(Err("A session settings update is already pending".into()));
            return;
        }
        let deadline = Instant::now() + APPLY_TIMEOUT;
        let result = tokio::time::timeout_at(deadline, submit(context, (&method, &params)))
            .await
            .unwrap_or_else(|_| Err(UpstreamError::Io("Timed out submitting session settings".into())));
        match result {
            Ok(params) => {
                eprintln!("[internal-codex-worker] stage=settings_update status=queued");
                self.pending = Some(PendingUpdate {
                    method,
                    params,
                    response,
                    deadline,
                });
            }
            Err(error) => {
                eprintln!(
                    "[internal-codex-worker] stage=settings_update status=error detail={}",
                    crate::diagnostics::safe_detail(&error.to_string())
                );
                let _ = response.send(Err(error.to_string()));
            }
        }
    }

    pub(super) fn observe(&mut self, event: &UpstreamEvent, settings: &mut SessionSettings) {
        let Some(pending) = self.pending.as_ref() else {
            return;
        };
        let UpstreamEvent::ServerNotification { method, params } = event else {
            return;
        };
        if params["threadId"] != pending.params["threadId"] {
            return;
        }
        if method == "error"
            && params
                .pointer("/error/message")
                .and_then(Value::as_str)
                .is_some_and(|message| message.starts_with("invalid thread settings override:"))
        {
            if let Some(message) = params.pointer("/error/message").and_then(Value::as_str) {
                self.fail(message);
            }
            return;
        }
        if method != "thread/settings/updated" {
            return;
        }
        let Some(snapshot) = params.get("threadSettings") else {
            return;
        };
        if !matches_requested(&pending.params, snapshot) {
            return;
        }
        settings.capture(snapshot);
        let pending = self.pending.take().expect("pending update was checked");
        eprintln!("[internal-codex-worker] stage=settings_update status=applied");
        let _ = pending
            .response
            .send(Ok(settings_mapping::response(&pending.method, settings)));
    }

    pub(super) fn fail(&mut self, message: &str) {
        if let Some(pending) = self.pending.take() {
            eprintln!(
                "[internal-codex-worker] stage=settings_update status=error detail={}",
                crate::diagnostics::safe_detail(message)
            );
            let _ = pending.response.send(Err(message.to_string()));
        }
        if let Some(super::BridgeCommand::Prompt { responder, .. }) = self.deferred_prompt.take() {
            let _ = responder.respond_with_error(super::to_sacp_error(message));
        }
    }
}

impl Drop for SettingsUpdate {
    fn drop(&mut self) {
        self.fail("Session closed before settings were applied");
    }
}

pub(super) fn is_update(method: &str) -> bool {
    matches!(
        method,
        "session/set_mode" | "session/set_config_option" | "session/set_model"
    )
}

async fn submit(
    context: (&UpstreamClient, &SessionSettings),
    input: (&str, &Value),
) -> Result<Value, UpstreamError> {
    let (upstream, settings) = context;
    let (request, _) = settings_mapping::request(input.0, input.1, settings)?;
    let params = request["params"].clone();
    let thread = params["threadId"]
        .as_str()
        .ok_or_else(|| UpstreamError::InvalidRequest("settings request has no thread id".into()))?;
    upstream.request_json_for_thread(thread, request).await?;
    Ok(params)
}

fn matches_requested(request: &Value, snapshot: &Value) -> bool {
    let Some(params) = request.as_object() else {
        return false;
    };
    params.iter().all(|(key, value)| match key.as_str() {
        "threadId" => true,
        "permissions" => snapshot.pointer("/activePermissionProfile/id") == Some(value),
        "serviceTier" if value.is_null() => snapshot[key].is_null() || snapshot[key] == "default",
        "serviceTier" if value == "fast" => snapshot[key] == "fast" || snapshot[key] == "priority",
        "collaborationMode" => ["/mode", "/settings/model", "/settings/reasoning_effort"]
            .iter()
            .all(|path| snapshot[key].pointer(path) == value.pointer(path)),
        _ => snapshot.get(key) == Some(value),
    })
}
