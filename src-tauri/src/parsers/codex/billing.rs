use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize)]
pub(crate) struct Receipt {
    pub thread_id: String,
    pub turn_id: String,
    pub request_id: String,
    pub purpose: String,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub root_turn_id: Option<String>,
    #[serde(default)]
    pub goal_identity_known: bool,
}

#[derive(Clone, Default)]
pub(crate) struct Receipts {
    pub incomplete: bool,
    pub requests: Vec<Receipt>,
    pub message_turns: HashMap<String, String>,
    pub turn_goals: HashMap<String, String>,
    pub tool_turns: HashMap<String, String>,
}

const RECEIPT_CACHE_LIMIT: usize = 256;
const MAX_REQUEST_ID_BYTES: usize = 64;
type Revision = (u64, Option<SystemTime>, u64, Option<SystemTime>);
type ReceiptCache = HashMap<(PathBuf, String), (Revision, Receipts)>;

fn cache() -> &'static Mutex<ReceiptCache> {
    static CACHE: OnceLock<Mutex<ReceiptCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn read(path: &Path, session_id: &str) -> Option<Receipts> {
    let meta = std::fs::metadata(path).ok()?;
    let billing = std::fs::metadata(path.with_extension("billing.jsonl")).ok();
    let revision = (
        meta.len(),
        meta.modified().ok(),
        billing.as_ref().map_or(0, |value| value.len()),
        billing.and_then(|value| value.modified().ok()),
    );
    let key = (path.to_path_buf(), session_id.to_string());
    if let Some((previous, receipts)) = cache().lock().ok()?.get(&key) {
        if *previous == revision {
            return Some(receipts.clone());
        }
    }
    let receipts = parse(path, session_id)?;
    let mut cached = cache().lock().ok()?;
    if cached.len() >= RECEIPT_CACHE_LIMIT && !cached.contains_key(&key) {
        cached.clear();
    }
    cached.insert(key, (revision, receipts.clone()));
    Some(receipts)
}

fn parse(path: &Path, session_id: &str) -> Option<Receipts> {
    let file = std::fs::File::open(path).ok()?;
    let mut result = Receipts::default();
    let mut active_goal = None;
    let mut turn_goals = HashMap::new();
    for line in BufReader::new(file).lines() {
        let line = line.ok()?;
        if !line.contains("\"item_completed\"")
            && !line.contains("\"thread_goal_updated\"")
            && !line.contains("\"task_started\"")
            && !line.contains("\"turn_context\"")
        {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        observe_goal_scope(&record, &mut active_goal, &mut turn_goals);
        if record.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let Some(payload) = record.get("payload") else {
            continue;
        };
        match payload.get("type").and_then(Value::as_str) {
            Some("item_completed") => {
                record_message_turn(payload, session_id, &mut result);
                if let (Some(item), Some(turn)) = (
                    payload.get("item"),
                    payload.get("turn_id").and_then(Value::as_str),
                ) {
                    if let Some(id) = item.get("id").and_then(Value::as_str) {
                        result.tool_turns.insert(id.into(), turn.into());
                    }
                }
            }
            _ => {}
        }
    }
    read_requests(
        &path.with_extension("billing.jsonl"),
        session_id,
        &mut result,
    );
    // 仅真实实例 ID 可建立归属；旧快照的秒级创建时间不能充当唯一身份。
    let mut ambiguous = HashSet::new();
    for receipt in &result.requests {
        if receipt.task_id.is_none() {
            ambiguous.insert(receipt.turn_id.clone());
        }
        if let Some(goal) = &receipt.task_id {
            if turn_goals
                .insert(receipt.turn_id.clone(), goal.clone())
                .is_some_and(|previous| previous != *goal)
            {
                ambiguous.insert(receipt.turn_id.clone());
            }
        }
    }
    for turn in ambiguous {
        turn_goals.remove(&turn);
    }
    result.turn_goals = turn_goals;
    Some(result)
}

fn observe_goal_scope(
    record: &Value,
    active: &mut Option<String>,
    turns: &mut HashMap<String, String>,
) {
    let Some(payload) = record.get("payload") else {
        return;
    };
    if payload.get("type").and_then(Value::as_str) == Some("thread_goal_updated") {
        *active = payload
            .get("goal")
            .filter(|goal| goal.get("status").and_then(Value::as_str) == Some("active"))
            .and_then(|goal| {
                goal.get("goal_id")
                    .or_else(|| goal.get("goalId"))
                    .or_else(|| goal.get("id"))
            })
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    let boundary = record.get("type").and_then(Value::as_str) == Some("turn_context")
        || payload.get("type").and_then(Value::as_str) == Some("task_started");
    if boundary {
        if let (Some(id), Some(goal)) = (
            payload.get("turn_id").and_then(Value::as_str),
            active.as_ref(),
        ) {
            turns.entry(id.to_string()).or_insert_with(|| goal.clone());
        }
    }
}

fn record_message_turn(payload: &Value, session_id: &str, result: &mut Receipts) {
    if payload.get("thread_id").and_then(Value::as_str) != Some(session_id)
        || payload.pointer("/item/type").and_then(Value::as_str) != Some("AgentMessage")
    {
        return;
    }
    if let (Some(id), Some(turn_id)) = (
        payload.pointer("/item/id").and_then(Value::as_str),
        payload.get("turn_id").and_then(Value::as_str),
    ) {
        result
            .message_turns
            .insert(id.to_string(), turn_id.to_string());
    }
}

pub(crate) fn read_requests(path: &Path, session_id: &str, result: &mut Receipts) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let mut seen: HashSet<_> = result
        .requests
        .iter()
        .map(|receipt| receipt.request_id.clone())
        .collect();
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {
            result.incomplete = true;
            break;
        };
        let Ok(receipt) = serde_json::from_str::<Receipt>(&line) else {
            result.incomplete |= !line.trim().is_empty();
            continue;
        };
        if receipt.thread_id == session_id
            && (receipt.request_id.is_empty() || receipt.request_id.len() > MAX_REQUEST_ID_BYTES)
        {
            result.incomplete = true;
        }
        if receipt.thread_id == session_id
            && !receipt.request_id.is_empty()
            && receipt.request_id.len() <= MAX_REQUEST_ID_BYTES
            && seen.insert(receipt.request_id.clone())
        {
            result.requests.push(receipt);
        }
    }
}
