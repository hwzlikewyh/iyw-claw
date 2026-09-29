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

pub(super) async fn generate(
    handle: InProcessAppServerRequestHandle,
    input: TitleInput,
    channels: (Arc<Mutex<Option<String>>>, mpsc::Receiver<Value>),
) -> Result<(), String> {
    let (hidden, events) = channels;
    let result = tokio::time::timeout(TITLE_TIMEOUT, run(&handle, &input, (&hidden, events)))
        .await
        .unwrap_or_else(|_| Err("Codex native title generation timed out".into()));
    let id = hidden
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if let Some(id) = id {
        let _ = tokio::time::timeout(
            CLEANUP_TIMEOUT,
            call(&handle, "thread/unsubscribe", json!({"threadId": id})),
        )
        .await;
    }
    result
}

async fn run(
    handle: &InProcessAppServerRequestHandle,
    input: &TitleInput,
    channels: (&Mutex<Option<String>>, mpsc::Receiver<Value>),
) -> Result<(), String> {
    if has_name(handle, &input.source_thread).await? {
        return Ok(());
    }
    let (hidden, mut events) = channels;
    let temporary = start_temporary(handle, input, hidden).await?;
    let turn_id = start_turn(handle, &temporary, &input.prompt, true).await?;
    let first = collect(&mut events, &turn_id, true).await;
    let title = match first {
        // 模型服务不支持 outputSchema 时，在同一临时线程中去掉 schema 重试一次。
        Err(error) if error == SCHEMA_UNSUPPORTED => {
            eprintln!(
                "[internal-codex-worker] stage=native_title status=retry reason=schema_unsupported"
            );
            let turn_id = start_turn(handle, &temporary, &input.prompt, false).await?;
            collect(&mut events, &turn_id, false).await?
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
    let turn = call(handle, "turn/start", params).await?;
    let turn_id = turn
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .ok_or("Codex title turn has no id")?;
    Ok(turn_id.to_string())
}

async fn collect(
    events: &mut mpsc::Receiver<Value>,
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
