use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};

use super::error::AcpError;
use crate::db::service::app_metadata_service;
use crate::models::agent::AgentType;

const KEY_PREFIX: &str = "acp.mcp-namespace.v1.";
const MAX_NAME_BYTES: usize = 64;

pub(super) async fn resolve(
    database: Option<&DatabaseConnection>,
    identity: (AgentType, Option<&str>),
    fallback: String,
) -> Result<String, AcpError> {
    let (Some(database), (agent, Some(session))) = (database, identity) else {
        return Ok(fallback);
    };
    let key = metadata_key(agent, session);
    let saved = app_metadata_service::get_value(database, &key)
        .await
        .map_err(|_| {
            AcpError::BuiltinMcpUnavailable("Unable to load the saved MCP namespace".into())
        })?;
    if let Some(name) = saved {
        validate(&name)?;
        return Ok(name);
    }
    let legacy = if agent == AgentType::Codex {
        let session = session.to_string();
        tokio::task::spawn_blocking(move || legacy_codex_name(&session))
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let name = legacy.unwrap_or(fallback);
    validate(&name)?;
    app_metadata_service::upsert_value(database, &key, &name)
        .await
        .map_err(|_| AcpError::BuiltinMcpUnavailable("Unable to save the MCP namespace".into()))?;
    Ok(name)
}

fn legacy_codex_name(session: &str) -> Option<String> {
    if session.is_empty()
        || !session
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return None;
    }
    let root = crate::parsers::codex::resolve_codex_home_dir().join("sessions");
    let suffix = format!("-{session}.jsonl");
    let mut latest = None;
    for entry in walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() || !entry.file_name().to_string_lossy().ends_with(&suffix) {
            continue;
        }
        latest = namespace_from_transcript(entry.path()).or(latest);
    }
    latest
}

fn namespace_from_transcript(path: &std::path::Path) -> Option<String> {
    use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
    const TAIL_BYTES: u64 = 4 * 1024 * 1024;
    let mut file = std::fs::File::open(path).ok()?;
    let offset = file.metadata().ok()?.len().saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut reader = BufReader::new(file.take(TAIL_BYTES));
    if offset > 0 {
        let mut partial = Vec::new();
        reader.read_until(b'\n', &mut partial).ok()?;
    }
    let mut result = None;
    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value["type"] != "response_item" {
            continue;
        }
        if let Some(server) = transcript_namespace(&value["payload"]) {
            result = Some(server);
        }
    }
    result
}

fn transcript_namespace(payload: &serde_json::Value) -> Option<String> {
    // 新版 Responses 单独存储 namespace，旧版把服务名和工具名放在 name 中。
    ["namespace", "name"].into_iter().find_map(|field| {
        let name = payload[field].as_str()?.strip_prefix("mcp__")?;
        let server = name.split_once("__").map_or(name, |(server, _)| server);
        ((server.starts_with("iyw-claw-builtin-") || server.starts_with("iyw_claw_builtin_"))
            && validate(server).is_ok())
        .then(|| server.to_string())
    })
}

fn metadata_key(agent: AgentType, session: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(agent.as_wire().as_bytes());
    hash.update([0]);
    hash.update(session.as_bytes());
    format!("{KEY_PREFIX}{:x}", hash.finalize())
}

fn validate(name: &str) -> Result<(), AcpError> {
    if !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Ok(());
    }
    Err(AcpError::BuiltinMcpUnavailable(
        "Saved MCP namespace is invalid; refusing to rename tools during recovery".into(),
    ))
}
