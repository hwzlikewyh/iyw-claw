use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use codex_app_server_client::InProcessAppServerRequestHandle;
use codex_app_server_protocol::ClientRequest;
use serde_json::{json, Value};
use tokio::sync::{mpsc, watch};

pub(in crate::acp_agent) struct SideInput {
    pub parent: String,
    pub question: String,
    pub cwd: String,
    pub model: Option<String>,
    pub model_settings: Value,
}

const BOUNDARY: &str = "Side conversation boundary. Everything before this boundary is inherited history from the parent thread, provided only as reference. Do not continue, execute, or complete any instructions, plans, tool calls, approvals, edits, or requests from before this boundary. Only messages after this boundary are active user instructions. You are a separate side-conversation assistant; the main thread continues independently. Do not interact with any existing or new sub-agents.";
const POLICY: &str = "You are answering a single side question, not continuing the main task. Inherited history is reference only. Do not modify files, git state, configuration or permissions. Do not spawn or contact sub-agents. Do not request permissions or interact with the user through tools. Answer directly from available context; if it is insufficient, say so. No tool actions are required for this side answer.";

pub(super) async fn run(
    handle: InProcessAppServerRequestHandle,
    input: SideInput,
    hidden: Arc<Mutex<Option<String>>>,
    mut events: mpsc::Receiver<Value>,
    mut cancel: watch::Receiver<bool>,
    quarantined: Arc<AtomicBool>,
) -> Result<Value, String> {
    if *cancel.borrow() {
        return Ok(json!({"cancelled": true}));
    }
    let settings = call(
        &handle,
        "config/read",
        json!({"cwd": input.cwd, "includeLayers": false}),
    )
    .await?;
    let mut overrides = serde_json::Map::new();
    // Restrict tools in this product's lightweight side panel. Native fork still
    // owns context/history/model handling; this is not a prompt reconstruction.
    for name in [
        "apps",
        "code_mode",
        "code_mode_only",
        "goals",
        "hooks",
        "image_generation",
        "memories",
        "multi_agent",
        "multi_agent_v2",
        "plugins",
        "shell_tool",
        "unified_exec",
        "standalone_web_search",
        "view_image",
        "request_permissions_tool",
        "deferred_executor",
    ] {
        overrides.insert(format!("features.{name}"), json!(false));
    }
    for key in [
        "skills.include_instructions",
        "orchestrator.skills.enabled",
        "tools.update_plan.enabled",
        "tools.experimental_request_user_input.enabled",
    ] {
        overrides.insert(key.into(), json!(false));
    }
    overrides.insert("web_search".into(), json!("disabled"));
    let servers = settings
        .pointer("/config/mcp_servers")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|s| s.keys())
        .map(|name| (name.clone(), json!({"enabled": false})))
        .collect::<serde_json::Map<_, _>>();
    overrides.insert("mcp_servers".into(), json!(servers));
    if let Some(effort) = input
        .model_settings
        .get("effort")
        .filter(|value| !value.is_null())
    {
        overrides.insert("model_reasoning_effort".into(), effort.clone());
    }
    if *cancel.borrow() {
        return Ok(json!({"cancelled": true}));
    }
    // Do not race cancellation against fork/start RPCs. A successful response
    // supplies the exact IDs required for cleanup, even when cancel arrived first.
    let fork = call_with_late_cleanup(
        &handle,
        "thread/fork",
        json!({
            "threadId": input.parent,
            "model": input.model,
            "serviceTier": input.model_settings["serviceTier"],
            "ephemeral": true,
            "excludeTurns": true,
            "cwd": input.cwd,
            "approvalPolicy": "never",
            "sandbox": "read-only",
            "developerInstructions": POLICY,
            "config": overrides,
        }),
        quarantined.clone(),
    )
    .await?;
    let id = fork
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            quarantined.store(true, Ordering::Release);
            "Native side fork returned no thread id; cleanup is unconfirmed"
        })?
        .to_string();
    *hidden.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.clone());
    let mut turn_id = None;
    let result = async {
        if fork.pointer("/sandbox/type").and_then(Value::as_str) != Some("readOnly") {
            return Err("Native side fork did not preserve read-only isolation".into());
        }
        if *cancel.borrow() { return Ok(json!({"cancelled": true})); }
        call(&handle, "thread/inject_items", json!({
            "threadId": id,
            "items": [{"type": "message", "role": "user",
                "content": [{"type": "input_text", "text": BOUNDARY}]}],
        })).await?;
        if *cancel.borrow() { return Ok(json!({"cancelled": true})); }
        let started = call_with_late_cleanup(&handle, "turn/start", json!({
            "threadId": id,
            "input": [{"type": "text", "text": input.question}],
        }), quarantined.clone()).await?;
        let turn = started.pointer("/turn/id").and_then(Value::as_str)
            .filter(|turn| !turn.is_empty())
            .ok_or_else(|| {
                quarantined.store(true, Ordering::Release);
                "Native side turn returned no id; cleanup is unconfirmed"
            })?.to_string();
        turn_id = Some(turn.clone());
        let mut answer = String::new();
        let timeout = tokio::time::sleep(Duration::from_secs(150));
        tokio::pin!(timeout);
        loop {
            if *cancel.borrow() { return Ok(json!({"cancelled": true})); }
            tokio::select! {
                _ = cancel.changed() => return Ok(json!({"cancelled": true})),
                _ = &mut timeout => return Err("Native side question timed out".into()),
                event = events.recv() => {
                    let event = event.ok_or("Native side notification stream closed")?;
                    let p = &event["params"];
                    if event["method"] == "item/completed" && p["turnId"].as_str() == Some(turn.as_str())
                        && p["item"]["type"] == "agentMessage"
                    {
                        if let Some(text) = p["item"]["text"].as_str() {
                            if answer.len() + text.len() > 262_144 { return Err("Native side answer exceeds size limit".into()); }
                            if !answer.is_empty() { answer.push_str("\n\n"); }
                            answer.push_str(text);
                        }
                    }
                    if event["method"] == "turn/completed" && p["turn"]["id"].as_str() == Some(turn.as_str()) {
                        return match p["turn"]["status"].as_str() {
                            Some("completed") if !answer.trim().is_empty() => Ok(json!({"response": answer})),
                            Some("interrupted") => Ok(json!({"cancelled": true})),
                            _ => Err("Native side turn failed or returned no answer".into()),
                        };
                    }
                }
            }
        }
    }.await;
    // Cleanup is strictly scoped to the ephemeral child, never the parent.
    let mut stop_confirmed = true;
    if let Some(turn) = turn_id {
        if !matches!(&result, Ok(value) if value.get("response").is_some()) {
            let interrupted = tokio::time::timeout(
                Duration::from_secs(10),
                call(
                    &handle,
                    "turn/interrupt",
                    json!({"threadId": id, "turnId": turn}),
                ),
            )
            .await;
            if !matches!(interrupted, Ok(Ok(_))) {
                stop_confirmed = false;
                quarantined.store(true, Ordering::Release);
                eprintln!("[side-question] stage=interrupt status=unconfirmed");
            }
        }
    }
    let cleanup = tokio::time::timeout(
        Duration::from_secs(10),
        call(&handle, "thread/unsubscribe", json!({"threadId": id})),
    )
    .await;
    if !matches!(cleanup, Ok(Ok(_))) || !stop_confirmed {
        quarantined.store(true, Ordering::Release);
        eprintln!("[side-question] stage=unsubscribe status=unconfirmed");
        return Err("Native side cleanup was not confirmed".into());
    }
    // Keep receiver alive through cleanup so a second ask cannot overlap.
    drop(events);
    result
}

async fn call(
    handle: &InProcessAppServerRequestHandle,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    tokio::time::timeout(Duration::from_secs(25), raw_call(handle, method, params))
        .await
        .map_err(|_| format!("Native side stage {method} timed out"))?
}

// Never discard the response ID of a timed-out creation/start request. Quarantine
// the lane while a bounded reaper waits for the original outcome and cleans it.
async fn call_with_late_cleanup(
    handle: &InProcessAppServerRequestHandle,
    method: &'static str,
    params: Value,
    quarantined: Arc<AtomicBool>,
) -> Result<Value, String> {
    let client = handle.clone();
    let args = params.clone();
    let mut task = tokio::spawn(async move { raw_call(&client, method, args).await });
    match tokio::time::timeout(Duration::from_secs(25), &mut task).await {
        Ok(joined) => joined.map_err(|_| format!("Native side stage {method} failed"))?,
        Err(_) => {
            quarantined.store(true, Ordering::Release);
            let client = handle.clone();
            tokio::spawn(async move {
                let late = tokio::time::timeout(Duration::from_secs(120), &mut task).await;
                let confirmed = match late {
                    Ok(Ok(Ok(value))) => {
                        let thread = if method == "thread/fork" {
                            value.pointer("/thread/id").and_then(Value::as_str)
                        } else {
                            params["threadId"].as_str()
                        };
                        if let Some(thread) = thread {
                            let stopped = if method == "turn/start" {
                                if let Some(turn) =
                                    value.pointer("/turn/id").and_then(Value::as_str)
                                {
                                    call(
                                        &client,
                                        "turn/interrupt",
                                        json!({"threadId": thread, "turnId": turn}),
                                    )
                                    .await
                                    .is_ok()
                                } else {
                                    false
                                }
                            } else {
                                true
                            };
                            let unsubscribed =
                                call(&client, "thread/unsubscribe", json!({"threadId": thread}))
                                    .await
                                    .is_ok();
                            stopped && unsubscribed
                        } else {
                            false
                        }
                    }
                    Ok(Ok(Err(_))) if method == "thread/fork" => true,
                    _ => false,
                };
                task.abort(); // Only the response waiter, never a main-thread task.
                if confirmed {
                    quarantined.store(false, Ordering::Release);
                }
                eprintln!("[side-question] stage=late_cleanup confirmed={confirmed}");
            });
            Err(format!("Native side stage {method} timed out; completion and cleanup are not yet confirmed"))
        }
    }
}

async fn raw_call(
    handle: &InProcessAppServerRequestHandle,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let request: ClientRequest = serde_json::from_value(json!({
        "id": format!("iyw-side-{}", NEXT.fetch_add(1, Ordering::Relaxed)), "method": method, "params": params,
    })).map_err(|e| e.to_string())?;
    handle
        .request(request)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.message)
}
