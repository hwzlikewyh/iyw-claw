use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use codex_app_server_client::InProcessAppServerRequestHandle;
use codex_app_server_protocol::ClientRequest;
use serde_json::{json, Value};
use tokio::sync::mpsc;

const TITLE_TIMEOUT: Duration = Duration::from_secs(30);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
// 调用方放弃等待后，turn/start 最多再等这么久拿到 turn id，以便停止晚到的轮次。
const TURN_START_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_TITLE_CHARS: usize = 36;
const MAX_RESPONSE_BYTES: usize = 8 * 1024;
// collect() 在模型服务拒绝 outputSchema 时返回的内部标记，run() 据此去掉 schema 重试一次。
const SCHEMA_UNSUPPORTED: &str = "Codex title output schema is unsupported";
// 结构化输出被拒绝时，失败文本同时包含格式字段和不可用描述。
const SCHEMA_MARKERS: &[&str] = &[
    "text.format",
    "json_schema",
    "response_format",
    "output_schema",
];
const UNSUPPORTED_MARKERS: &[&str] = &["unavailable", "unsupported", "not supported"];
// 无 schema 回复中可能包裹标题的成对引号。
const TITLE_QUOTES: &[(char, char)] = &[
    ('"', '"'),
    ('\'', '\''),
    ('“', '”'),
    ('‘', '’'),
    ('「', '」'),
    ('『', '』'),
];
const TITLE_DISABLED_FEATURES: &[&str] = &[
    "apps",
    "code_mode",
    "code_mode_only",
    "context_management",
    "current_time_reminder",
    "deferred_executor",
    "enable_fanout",
    "goals",
    "hooks",
    "image_generation",
    "memories",
    "multi_agent",
    "multi_agent_v2",
    "plugins",
    "request_permissions_tool",
    "shell_snapshot",
    "shell_tool",
    "standalone_web_search",
    "token_budget",
    "tool_suggest",
    "unified_exec",
    "view_image",
];

pub(in crate::acp_agent) struct TitleInput {
    pub source_thread: String,
    pub prompt: String,
    pub model: Option<String>,
    pub cwd: String,
}

/// 标题临时线程上仍在运行的轮次。超时、出错或会话结束时据此先停止这一轮，再退订临时线程；
/// 清理开始（`closed`）之后才拿到 turn id 的 turn/start，由启动任务自行停止。
#[derive(Default)]
pub(super) struct TitleLane {
    turn: Option<(String, String)>,
    closed: bool,
}

fn lock_lane(lane: &Mutex<TitleLane>) -> std::sync::MutexGuard<'_, TitleLane> {
    lane.lock().unwrap_or_else(|error| error.into_inner())
}

/// 记录刚启动的轮次；清理已经开始时返回 false，调用方必须自行停止这一轮。
fn record_turn(lane: &Mutex<TitleLane>, thread: &str, turn: &str) -> bool {
    let mut lane = lock_lane(lane);
    if lane.closed {
        return false;
    }
    lane.turn = Some((thread.to_string(), turn.to_string()));
    true
}

/// 收到这一轮的 turn/completed 之后就不需要再停止它。
fn finish_turn(lane: &Mutex<TitleLane>, turn: &str) {
    let mut lane = lock_lane(lane);
    if lane.turn.as_ref().is_some_and(|(_, id)| id == turn) {
        lane.turn = None;
    }
}

/// 开始清理并取出仍需停止的轮次；之后启动的轮次由启动任务自行停止。
fn close_lane(lane: &Mutex<TitleLane>) -> Option<(String, String)> {
    let mut lane = lock_lane(lane);
    lane.closed = true;
    lane.turn.take()
}

pub(super) async fn generate(
    handle: InProcessAppServerRequestHandle,
    input: TitleInput,
    lanes: (Arc<Mutex<Option<String>>>, Arc<Mutex<TitleLane>>),
    events: mpsc::Receiver<Value>,
) -> Result<(), String> {
    let (hidden, lane) = lanes;
    let result = tokio::time::timeout(TITLE_TIMEOUT, run(&handle, &input, (&hidden, &lane), events))
        .await
        .unwrap_or_else(|_| Err("Codex native title generation timed out".into()));
    // 成功、出错和超时都走同一清理：先停止仍在运行的轮次，再退订临时线程。
    cleanup(&handle, &hidden, &lane).await;
    result
}

async fn run(
    handle: &InProcessAppServerRequestHandle,
    input: &TitleInput,
    lanes: (&Mutex<Option<String>>, &Arc<Mutex<TitleLane>>),
    mut events: mpsc::Receiver<Value>,
) -> Result<(), String> {
    if has_name(handle, &input.source_thread).await? {
        return Ok(());
    }
    let (hidden, lane) = lanes;
    let temporary = start_temporary(handle, input, hidden).await?;
    let turn_id = start_turn(handle, lane, &temporary, &input.prompt, true).await?;
    let first = collect(&mut events, lane, &turn_id, true).await;
    let title = match first {
        // 模型服务不支持 outputSchema 时，在同一临时线程中去掉 schema 重试一次。
        Err(error) if error == SCHEMA_UNSUPPORTED => {
            eprintln!(
                "[internal-codex-worker] stage=native_title status=retry reason=schema_unsupported"
            );
            let turn_id = start_turn(handle, lane, &temporary, &input.prompt, false).await?;
            collect(&mut events, lane, &turn_id, false).await?
        }
        result => result?,
    };
    // 生成期间可能发生手动命名，写入前再次核对。
    if has_name(handle, &input.source_thread).await? {
        return Ok(());
    }
    call(
        handle,
        "thread/name/set",
        json!({"threadId": input.source_thread, "name": title}),
    )
    .await?;
    Ok(())
}

/// 清理顺序：先停止仍在运行的轮次，再退订临时线程。只退订不会停下正在运行的轮次：app server
/// 要等它结束才卸载线程，期间模型请求还会继续重试。
fn cleanup_calls(
    turn: Option<(String, String)>,
    thread: Option<String>,
) -> Vec<(&'static str, Value)> {
    let mut calls = Vec::new();
    if let Some((thread, turn)) = turn {
        calls.push(("turn/interrupt", json!({"threadId": thread, "turnId": turn})));
    }
    if let Some(thread) = thread {
        calls.push(("thread/unsubscribe", json!({"threadId": thread})));
    }
    calls
}

async fn run_cleanup(handle: &InProcessAppServerRequestHandle, calls: Vec<(&'static str, Value)>) {
    for (method, params) in calls {
        let done = tokio::time::timeout(CLEANUP_TIMEOUT, call(handle, method, params)).await;
        if !matches!(done, Ok(Ok(_))) {
            eprintln!(
                "[internal-codex-worker] stage=native_title_cleanup method={method} status=unconfirmed"
            );
        }
    }
}

/// 结束标题请求，成功、出错、超时和会话结束共用；可以重复调用。
pub(super) async fn cleanup(
    handle: &InProcessAppServerRequestHandle,
    hidden: &Mutex<Option<String>>,
    lane: &Mutex<TitleLane>,
) {
    let turn = close_lane(lane);
    let thread = hidden
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    run_cleanup(handle, cleanup_calls(turn, thread)).await;
}

async fn has_name(
    handle: &InProcessAppServerRequestHandle,
    thread_id: &str,
) -> Result<bool, String> {
    let response = call(
        handle,
        "thread/read",
        json!({"threadId": thread_id, "includeTurns": false}),
    )
    .await?;
    Ok(response
        .pointer("/thread/name")
        .and_then(Value::as_str)
        .is_some_and(|name| !name.trim().is_empty()))
}

async fn start_temporary(
    handle: &InProcessAppServerRequestHandle,
    input: &TitleInput,
    hidden: &Mutex<Option<String>>,
) -> Result<String, String> {
    let config = call(
        handle,
        "config/read",
        json!({"cwd": input.cwd, "includeLayers": false}),
    )
    .await?;
    let response = call(
        handle,
        "thread/start",
        temporary_params(input, &config["config"])?,
    )
    .await?;
    let temporary = response
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .ok_or("Codex title thread has no id")?
        .to_string();
    *hidden.lock().unwrap_or_else(|error| error.into_inner()) = Some(temporary.clone());
    if response.pointer("/sandbox/type").and_then(Value::as_str) != Some("readOnly") {
        return Err("Codex title thread did not preserve read-only isolation".into());
    }
    Ok(temporary)
}

fn temporary_params(input: &TitleInput, settings: &Value) -> Result<Value, String> {
    let mut config = serde_json::Map::new();
    for feature in TITLE_DISABLED_FEATURES {
        config.insert(format!("features.{feature}"), json!(false));
    }
    for key in [
        "orchestrator.skills.enabled",
        "skills.include_instructions",
        "tools.experimental_request_user_input.enabled",
        "tools.update_plan.enabled",
    ] {
        config.insert(key.to_string(), json!(false));
    }
    config.insert("features.token_budget".into(), json!({
        "enabled": false, "use_history_notes_extension": false,
    }));
    config.insert("web_search".into(), json!("disabled"));
    config.insert(
        "mcp_servers".into(),
        super::super::isolated_config::disabled_mcp_servers(settings)?,
    );
    Ok(json!({"model": input.model.as_ref().map(|s| json!(s)).unwrap_or_else(|| settings["model"].clone()),
        "modelProvider": settings["model_provider"], "cwd": input.cwd, "approvalPolicy": "never",
        "sandbox": "read-only", "runtimeWorkspaceRoots": [], "ephemeral": true,
        "threadSource": "feature:system", "environments": [], "dynamicTools": [],
        "selectedCapabilityRoots": [], "config": config}))
}

async fn start_turn(
    handle: &InProcessAppServerRequestHandle,
    lane: &Arc<Mutex<TitleLane>>,
    thread_id: &str,
    prompt: &str,
    structured: bool,
) -> Result<String, String> {
    let mut params = json!({
        "threadId": thread_id, "input": [{"type": "text", "text": title_prompt(prompt)}]
    });
    if structured {
        params["outputSchema"] = json!({
            "type": "object", "additionalProperties": false, "required": ["title"],
            "properties": {
                "title": {"type": "string", "minLength": 1, "maxLength": MAX_TITLE_CHARS}
            }
        });
    }
    // turn/start 放进独立任务：调用方被取消（超时或会话结束）时也能拿到 turn id；清理已经开始时，
    // 由这个任务自己停止这一轮并退订临时线程。
    let client = handle.clone();
    let lane = Arc::clone(lane);
    let thread = thread_id.to_string();
    let task = tokio::spawn(async move {
        let started = tokio::time::timeout(TURN_START_TIMEOUT, call(&client, "turn/start", params))
            .await
            .map_err(|_| "Codex title turn start timed out".to_string())??;
        let turn_id = started
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or("Codex title turn has no id")?
            .to_string();
        if !record_turn(&lane, &thread, &turn_id) {
            let calls = cleanup_calls(Some((thread.clone(), turn_id)), Some(thread));
            run_cleanup(&client, calls).await;
            return Err("Codex title turn started after cleanup".to_string());
        }
        Ok::<_, String>(turn_id)
    });
    task.await
        .map_err(|_| "Codex title turn start task failed".to_string())?
}

async fn collect(
    events: &mut mpsc::Receiver<Value>,
    lane: &Mutex<TitleLane>,
    turn_id: &str,
    structured: bool,
) -> Result<String, String> {
    let mut text = None;
    while let Some(event) = events.recv().await {
        let params = &event["params"];
        if event["method"] == "item/completed"
            && params["turnId"] == turn_id
            && params["item"]["type"] == "agentMessage"
        {
            if let Some(value) = params["item"]["text"].as_str() {
                if value.len() > MAX_RESPONSE_BYTES {
                    return Err("Codex title response exceeds limit".into());
                }
                text = Some(value.to_string());
            }
        }
        if event["method"] == "turn/completed" && params["turn"]["id"] == turn_id {
            // 这一轮已经结束（无论成败），清理时不必再停止它。
            finish_turn(lane, turn_id);
            if params["turn"]["status"] != "completed" {
                if structured && output_schema_unsupported(&params["turn"]["error"]) {
                    return Err(SCHEMA_UNSUPPORTED.into());
                }
                return Err("Codex title turn failed".into());
            }
            let reply = text.as_deref().ok_or("Codex title response is empty")?;
            let title = if structured {
                let value: Value = serde_json::from_str(reply)
                    .map_err(|_| "Codex title response is not valid JSON")?;
                normalize_title(value["title"].as_str().unwrap_or_default())
            } else {
                plain_title(reply)
            };
            return if title.is_empty() {
                Err("Codex title response has no title".into())
            } else {
                Ok(title)
            };
        }
    }
    Err("Codex title notification channel closed".into())
}

fn normalize_title(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_TITLE_CHARS)
        .collect()
}

// 无 schema 回复：先去掉 ``` 围栏行；以 { 开头时只接受带字符串 title 的 JSON 对象，
// 其余取首个非空行并去掉成对引号。
fn plain_title(reply: &str) -> String {
    let body = reply
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n");
    let body = body.trim();
    if body.starts_with('{') {
        // JSON 解析失败或没有 title 时不产出标题，避免把 "{" 这类片段当成标题。
        return serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| value.get("title")?.as_str().map(normalize_title))
            .unwrap_or_default();
    }
    let line = body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    normalize_title(strip_quotes(line))
}

fn strip_quotes(line: &str) -> &str {
    TITLE_QUOTES
        .iter()
        .find_map(|&(open, close)| line.strip_prefix(open)?.strip_suffix(close))
        .unwrap_or(line)
}

fn output_schema_unsupported(error: &Value) -> bool {
    let detail = error["message"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    SCHEMA_MARKERS.iter().any(|m| detail.contains(m))
        && UNSUPPORTED_MARKERS.iter().any(|m| detail.contains(m))
}

async fn call(
    handle: &InProcessAppServerRequestHandle,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let request: ClientRequest = serde_json::from_value(json!({
        "id": format!("iyw-native-title-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)), "method": method, "params": params,
    })).map_err(|error| error.to_string())?;
    handle
        .request(request)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.message)
}

fn title_prompt(prompt: &str) -> String {
    format!("Generate a concise, single-line task title of at most {MAX_TITLE_CHARS} characters and under five words where possible. Start with an imperative verb. Write in the user's language. Do not use quotes, markdown, or trailing punctuation. Do not answer the request.\n\nUser prompt:\n{prompt}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn methods(calls: &[(&'static str, Value)]) -> Vec<&'static str> {
        calls.iter().map(|(method, _)| *method).collect()
    }

    #[test]
    fn cleanup_interrupts_running_turn_before_unsubscribe() {
        let lane = Mutex::new(TitleLane::default());
        assert!(record_turn(&lane, "thread-1", "turn-1"));
        let calls = cleanup_calls(close_lane(&lane), Some("thread-1".to_string()));
        assert_eq!(methods(&calls), ["turn/interrupt", "thread/unsubscribe"]);
        assert_eq!(calls[0].1, json!({"threadId": "thread-1", "turnId": "turn-1"}));
        assert_eq!(calls[1].1, json!({"threadId": "thread-1"}));
    }

    #[test]
    fn cleanup_skips_interrupt_after_turn_completed() {
        let lane = Mutex::new(TitleLane::default());
        assert!(record_turn(&lane, "thread-1", "turn-1"));
        finish_turn(&lane, "turn-1");
        let calls = cleanup_calls(close_lane(&lane), Some("thread-1".to_string()));
        assert_eq!(methods(&calls), ["thread/unsubscribe"]);
    }

    #[test]
    fn completion_of_an_earlier_turn_keeps_the_running_turn() {
        let lane = Mutex::new(TitleLane::default());
        assert!(record_turn(&lane, "thread-1", "turn-2"));
        finish_turn(&lane, "turn-1");
        assert_eq!(
            close_lane(&lane),
            Some(("thread-1".to_string(), "turn-2".to_string()))
        );
    }

    #[test]
    fn turn_started_after_cleanup_must_stop_itself() {
        let lane = Mutex::new(TitleLane::default());
        assert_eq!(close_lane(&lane), None);
        assert!(!record_turn(&lane, "thread-1", "turn-1"));
        assert_eq!(close_lane(&lane), None);
    }
}
