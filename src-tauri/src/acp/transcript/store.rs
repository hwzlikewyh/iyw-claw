use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::models::{AgentType, MessageTurn};

use super::{TranscriptData, TranscriptHeader, TranscriptRecord};

const MAX_CONTINUATION_DEPTH: usize = 64;

fn write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn transcript_path_in(root: &Path, agent_dir: &str, session_id: &str) -> Option<PathBuf> {
    (safe_component(agent_dir, 64) && safe_component(session_id, 128))
        .then(|| root.join(agent_dir).join(format!("{session_id}.jsonl")))
}

pub fn write_header(agent: AgentType, header: &TranscriptHeader) -> std::io::Result<()> {
    write_header_in(
        &crate::paths::iyw_claw_acp_transcripts_root(),
        agent,
        header,
    )
}

pub fn write_header_in(
    root: &Path,
    agent: AgentType,
    header: &TranscriptHeader,
) -> std::io::Result<()> {
    if header.agent != agent || header.session_id.is_empty() {
        return Err(invalid_input("transcript header identity mismatch"));
    }
    let path = path_for(root, agent, &header.session_id)?;
    append_record(&path, &TranscriptRecord::header(header.clone()), true)
}

pub fn append_turn(agent: AgentType, session_id: &str, turn: MessageTurn) -> std::io::Result<()> {
    append_turn_in(
        &crate::paths::iyw_claw_acp_transcripts_root(),
        agent,
        session_id,
        turn,
    )
}

pub fn append_turn_in(
    root: &Path,
    agent: AgentType,
    session_id: &str,
    turn: MessageTurn,
) -> std::io::Result<()> {
    let path = path_for(root, agent, session_id)?;
    append_record(&path, &TranscriptRecord::turn(turn), false)
}

pub fn read_header_in(root: &Path, agent_dir: &str, session_id: &str) -> Option<TranscriptHeader> {
    read_transcript_in(root, agent_dir, session_id).header
}

pub fn read_chain_in(root: &Path, agent: AgentType, session_id: &str) -> TranscriptData {
    let agent_dir = crate::acp::registry::registry_id_for(agent);
    let mut seen = HashSet::new();
    let mut chain = Vec::new();
    let mut current = session_id.to_string();

    for _ in 0..MAX_CONTINUATION_DEPTH {
        if !seen.insert(current.clone()) {
            break;
        }
        let mut data = read_transcript_in(root, agent_dir, &current);
        let Some(header) = data
            .header
            .as_ref()
            .filter(|header| header.agent == agent && header.session_id == current)
        else {
            data.invalid_lines += data.turns.len();
            data.turns.clear();
            chain.push(data);
            break;
        };
        let previous = header.continues_from.clone();
        chain.push(data);
        let Some(previous) = previous else { break };
        current = previous;
    }

    let mut merged = TranscriptData::default();
    for data in chain.into_iter().rev() {
        merged.merge(data);
    }
    merged
}

pub fn list_session_ids_in(root: &Path, agent_dir: &str) -> Vec<String> {
    list_components(root.join(agent_dir), Some("jsonl"))
}

pub fn list_agent_dirs_in(root: &Path) -> Vec<String> {
    list_components(root.to_path_buf(), None)
}

pub fn superseded_session_ids_in(root: &Path, agent_dir: &str) -> HashSet<String> {
    list_session_ids_in(root, agent_dir)
        .into_iter()
        .filter_map(|session_id| read_header_in(root, agent_dir, &session_id)?.continues_from)
        .collect()
}

fn read_transcript_in(root: &Path, agent_dir: &str, session_id: &str) -> TranscriptData {
    let Some(path) = transcript_path_in(root, agent_dir, session_id) else {
        return TranscriptData::default();
    };
    let Ok(bytes) = std::fs::read(path) else {
        return TranscriptData::default();
    };
    parse_transcript(&String::from_utf8_lossy(&bytes))
}

fn parse_transcript(content: &str) -> TranscriptData {
    let mut data = TranscriptData::default();
    let mut index = HashMap::new();
    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        match serde_json::from_str::<TranscriptRecord>(line) {
            Ok(record) => data.apply(record, &mut index),
            Err(_) => data.invalid_lines += 1,
        }
    }
    data
}

fn append_record(path: &Path, record: &TranscriptRecord, header_only: bool) -> std::io::Result<()> {
    let serialized = serde_json::to_vec(record).map_err(invalid_data)?;
    let _guard = write_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if header_only && path.metadata().is_ok_and(|metadata| metadata.len() > 0) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(&serialized)?;
    file.write_all(b"\n")
}

fn path_for(root: &Path, agent: AgentType, session_id: &str) -> std::io::Result<PathBuf> {
    transcript_path_in(
        root,
        crate::acp::registry::registry_id_for(agent),
        session_id,
    )
    .ok_or_else(|| invalid_input("unsafe ACP transcript path component"))
}

fn list_components(path: PathBuf, extension: Option<&str>) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut values = entries
        .filter_map(Result::ok)
        .filter(|entry| extension.is_none() == entry.path().is_dir())
        .filter_map(|entry| component_name(&entry.path(), extension))
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn component_name(path: &Path, extension: Option<&str>) -> Option<String> {
    let value = match extension {
        Some(expected) => {
            (path.extension()?.to_str()? == expected).then_some(())?;
            path.file_stem()?.to_str()?
        }
        None => path.file_name()?.to_str()?,
    };
    safe_component(value, 128).then(|| value.to_string())
}

fn safe_component(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn invalid_input(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
}

fn invalid_data(error: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error)
}
