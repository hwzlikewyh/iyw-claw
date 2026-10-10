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
}

#[derive(Clone, Default)]
pub(crate) struct Receipts {
    pub requests: Vec<Receipt>,
    pub message_turns: HashMap<String, String>,
}

const RECEIPT_CACHE_LIMIT: usize = 16;
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
    if cached.len() >= RECEIPT_CACHE_LIMIT {
        cached.clear();
    }
    cached.insert(key, (revision, receipts.clone()));
    Some(receipts)
}

fn parse(path: &Path, session_id: &str) -> Option<Receipts> {
    let file = std::fs::File::open(path).ok()?;
    let mut result = Receipts::default();
    for line in BufReader::new(file).lines() {
        let line = line.ok()?;
        if !line.contains("\"item_completed\"") {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if record.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let Some(payload) = record.get("payload") else {
            continue;
        };
        match payload.get("type").and_then(Value::as_str) {
            Some("item_completed") => record_message_turn(payload, session_id, &mut result),
            _ => {}
        }
    }
    read_requests(
        &path.with_extension("billing.jsonl"),
        session_id,
        &mut result,
    );
    Some(result)
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
            break;
        };
        let Ok(receipt) = serde_json::from_str::<Receipt>(&line) else {
            continue;
        };
        if receipt.thread_id == session_id
            && !receipt.request_id.is_empty()
            && receipt.request_id.len() <= 64
            && seen.insert(receipt.request_id.clone())
        {
            result.requests.push(receipt);
        }
    }
}
