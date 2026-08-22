use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use regex::Regex;
use walkdir::WalkDir;

use crate::models::*;
use crate::parsers::{
    folder_name_from_path, title_from_user_text, truncate_str, AgentParser, ParseError,
};

pub struct CodexParser {
    base_dir: PathBuf,
}

impl Default for CodexParser {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexParser {
    pub fn new() -> Self {
        let base_dir = resolve_codex_home_dir().join("sessions");
        Self { base_dir }
    }

    /// Load Codex's append-only session title index. The transcript remains the
    /// fallback source, so a missing/unreadable index or a malformed line is
    /// deliberately ignored. Later non-empty records for the same session win.
    pub(crate) fn load_thread_name_index(&self) -> HashMap<String, String> {
        let mut titles = HashMap::new();
        let Some(home_dir) = self.base_dir.parent() else {
            return titles;
        };
        let Ok(file) = fs::File::open(home_dir.join("session_index.jsonl")) else {
            return titles;
        };

        for line in BufReader::new(file).lines() {
            let line = match line {
                Ok(line) => line,
                Err(_) => break,
            };
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let session_id = value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty());
            let thread_name = value
                .get("thread_name")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty());
            if let (Some(id), Some(name)) = (session_id, thread_name) {
                titles.insert(id.to_string(), truncate_str(name, 100));
            }
        }

        titles
    }

    /// Test-only constructor that lets callers point the parser at a fixture
    /// directory instead of `~/.codex/sessions`.

    fn parse_jsonl_summary(
        &self,
        path: &PathBuf,
    ) -> Result<Option<ConversationSummary>, ParseError> {
        let file = fs::File::open(path)?;
        let reader = BufReader::new(file);

        let mut conversation_id: Option<String> = None;
        let mut cwd: Option<String> = None;
        let mut git_branch: Option<String> = None;
        let mut model: Option<String> = None;
        let mut title: Option<String> = None;
        let mut _cli_version: Option<String> = None;
        let mut first_timestamp: Option<DateTime<Utc>> = None;
        let mut last_timestamp: Option<DateTime<Utc>> = None;
        let mut message_count: u32 = 0;
        // Mirror the detail parser's leading-`/goal` fallback in the lightweight
        // list path: newer codex records `/goal` only as `thread_goal_updated` (no
        // `user_message`), so without this the sidebar/import entry is titleless
        // and under-counted. Decide it POSITIONALLY, exactly like the detail
        // parser: the goal is the opener iff no real user turn preceded it, and it
        // then supplies the title + one synthetic-turn count even when a LATER real
        // reply (e.g. "确认") exists. `has_real_user` tracks the same real-user-turn
        // sources detail uses (an `event_msg.user_message`, or an image-bearing
        // `response_item` user); `goal_objective` latches the first opening goal;
        // `goal_opens_session` snapshots whether it opened the session.
        let mut has_real_user = false;
        let mut goal_objective: Option<String> = None;
        let mut goal_opens_session = false;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.trim().is_empty() {
                continue;
            }

            let value: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let msg_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");

            if let Some(ts_str) = value.get("timestamp").and_then(|t| t.as_str()) {
                if let Ok(ts) = ts_str.parse::<DateTime<Utc>>() {
                    if first_timestamp.is_none() {
                        first_timestamp = Some(ts);
                    }
                    last_timestamp = Some(ts);
                }
            }

            match msg_type {
                "session_meta" => {
                    if let Some(payload) = value.get("payload") {
                        conversation_id = payload
                            .get("id")
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string());
                        cwd = payload
                            .get("cwd")
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string());
                        _cli_version = payload
                            .get("cli_version")
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string());
                        git_branch = payload
                            .get("git")
                            .and_then(|g| g.get("branch"))
                            .and_then(|b| b.as_str())
                            .map(|s| s.to_string());
                    }
                }
                "turn_context" if model.is_none() => {
                    model = value
                        .get("payload")
                        .and_then(|p| p.get("model"))
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string());
                }
                "event_msg" => {
                    if let Some(payload) = value.get("payload") {
                        let payload_type =
                            payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match payload_type {
                            "user_message" => {
                                message_count += 1;
                                has_real_user = true;
                                if title.is_none() {
                                    title = payload
                                        .get("message")
                                        .and_then(|m| m.as_str())
                                        .and_then(|text| extract_codex_title_candidate(text, true));
                                }
                            }
                            "agent_message" => {
                                message_count += 1;
                            }
                            "thread_goal_updated" => {
                                // Capture the first OPENING goal for the fallback,
                                // through the SAME shared mapping the detail parser
                                // uses — so the summary keys off exactly the objective
                                // the detail parser would synthesize from: only a
                                // `create_goal` (an active goal with an objective),
                                // never a `goal:null` clear, a blank objective, or a
                                // terminal-status goal.
                                if goal_objective.is_none() {
                                    if let Some(marker) = payload
                                        .get("goal")
                                        .and_then(crate::acp::codex_goal::goal_marker)
                                    {
                                        if marker.tool_name == "create_goal" {
                                            // Positional, mirroring the detail parser:
                                            // the goal opened the session iff no real
                                            // user turn preceded it. Claim the title
                                            // from the objective HERE, in stream order,
                                            // so a later `user_message` can't steal it
                                            // while a native `thread_name_updated` still
                                            // overrides it.
                                            goal_opens_session = !has_real_user;
                                            if goal_opens_session && title.is_none() {
                                                title = extract_codex_title_candidate(
                                                    &marker.objective,
                                                    true,
                                                );
                                            }
                                            goal_objective = Some(marker.objective);
                                        }
                                    }
                                }
                            }
                            "thread_name_updated" => {
                                // Codex native thread name — newest non-empty wins
                                // (parity with the detail parser). Accept both the
                                // rollout `thread_name` and the live `threadName`.
                                if let Some(name) = payload
                                    .get("thread_name")
                                    .or_else(|| payload.get("threadName"))
                                    .or_else(|| payload.get("name"))
                                    .and_then(|n| n.as_str())
                                    .map(str::trim)
                                    .filter(|n| !n.is_empty())
                                {
                                    title = Some(truncate_str(name, 100));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "response_item" => {
                    if let Some(payload) = value.get("payload") {
                        let payload_type =
                            payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        if payload_type == "message" {
                            let role = payload.get("role").and_then(|r| r.as_str()).unwrap_or("");
                            // The detail parser only turns an IMAGE-bearing
                            // `response_item` user into a real user turn
                            // (`extract_response_item_user_image_blocks`) and only
                            // titles from that same turn. Text-only `response_item`
                            // users are internal envelopes (`<environment_context>`,
                            // `<codex_internal_context>`, `<turn_aborted>`, …) or
                            // duplicates of `event_msg.user_message` — detail ignores
                            // them for BOTH the turn and the title, so the summary
                            // must too. Mirroring it here keeps the pure-`/goal`
                            // fallback (title + count) in exact sync and stops
                            // internal text from leaking into the list title.
                            if role == "user" && response_item_user_has_image(payload) {
                                has_real_user = true;
                                if title.is_none() {
                                    title = extract_codex_text_content(payload)
                                        .and_then(|t| extract_codex_title_candidate(&t, false));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let started_at = match first_timestamp {
            Some(ts) => ts,
            None => return Ok(None),
        };

        // Leading-`/goal` fallback, positional and mirroring the detail parser:
        // when a `/goal` opened the session (before any real user turn), the detail
        // view synthesizes a leading user message from the objective — so count
        // that one turn here, even when a LATER real reply exists, keeping the list
        // entry in sync with the opened conversation. The title was already claimed
        // in-loop (see the goal arm).
        if goal_opens_session {
            message_count += 1;
        }

        let id = conversation_id.unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

        let folder_path = cwd.clone();
        let folder_name = folder_path.as_ref().map(|p| folder_name_from_path(p));

        Ok(Some(ConversationSummary {
            id,
            agent_type: AgentType::Codex,
            folder_path,
            folder_name,
            title,
            started_at,
            ended_at: last_timestamp,
            message_count,
            model,
            git_branch,
            parent_id: None,
            parent_tool_use_id: None,
            delegation_call_id: None,
        }))
    }
}

pub(crate) fn resolve_codex_home_dir() -> PathBuf {
    resolve_codex_home_dir_from(std::env::var_os("CODEX_HOME"), dirs::home_dir())
}

fn resolve_codex_home_dir_from(
    codex_home_env: Option<std::ffi::OsString>,
    home_dir: Option<PathBuf>,
) -> PathBuf {
    codex_home_env
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir.unwrap_or_default().join(".codex"))
}

impl AgentParser for CodexParser {
    fn list_conversations(&self) -> Result<Vec<ConversationSummary>, ParseError> {
        let mut conversations = Vec::new();

        if !self.base_dir.exists() {
            return Ok(conversations);
        }

        // Apply this outside `summary_cache`: changing only session_index.jsonl
        // must refresh a title even when the rollout itself is unchanged.
        let indexed_titles = self.load_thread_name_index();

        for entry in WalkDir::new(&self.base_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path().to_path_buf();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let fname = path.file_name().unwrap_or_default().to_string_lossy();
            if !fname.starts_with("rollout-") {
                continue;
            }

            match super::summary_cache::get_or_parse(AgentType::Codex, &path, || {
                self.parse_jsonl_summary(&path)
            }) {
                Ok(Some(mut summary)) => {
                    if let Some(title) = indexed_titles.get(&summary.id) {
                        summary.title = Some(title.clone());
                    }
                    conversations.push(summary);
                }
                _ => continue,
            }
        }

        conversations.sort_by_key(|b| std::cmp::Reverse(b.started_at));
        Ok(conversations)
    }

    fn get_conversation(&self, conversation_id: &str) -> Result<ConversationDetail, ParseError> {
        if !self.base_dir.exists() {
            return Err(ParseError::ConversationNotFound(
                conversation_id.to_string(),
            ));
        }

        // Find the conversation file by walking the directory tree
        for entry in WalkDir::new(&self.base_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path().to_path_buf();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let fname = path.file_name().unwrap_or_default().to_string_lossy();
            if fname.contains(conversation_id) {
                let mut detail = self.parse_conversation_detail(&path, conversation_id)?;
                if let Some(title) = self.load_thread_name_index().get(conversation_id) {
                    detail.summary.title = Some(title.clone());
                }
                return Ok(detail);
            }
        }

        Err(ParseError::ConversationNotFound(
            conversation_id.to_string(),
        ))
    }
}

fn parse_codex_json_arg(payload: &serde_json::Value) -> Option<serde_json::Value> {
    let args = payload.get("arguments").or_else(|| payload.get("input"))?;
    if let Some(s) = args.as_str() {
        serde_json::from_str(s).ok()
    } else if args.is_object() || args.is_array() {
        Some(args.clone())
    } else {
        None
    }
}

fn parse_codex_json_output(payload: &serde_json::Value) -> Option<serde_json::Value> {
    let output = payload.get("output")?;
    if let Some(s) = output.as_str() {
        serde_json::from_str(s).ok()
    } else if output.is_object() || output.is_array() {
        Some(output.clone())
    } else {
        None
    }
}

fn clean_codex_exec_output(text: &str) -> String {
    let mut cmd_line: Option<&str> = None;
    let mut in_output = false;
    let mut output_lines = Vec::new();

    for line in text.lines() {
        if cmd_line.is_none() && line.starts_with("$ ") {
            cmd_line = Some(line);
            continue;
        }
        if line == "Output:" || line == "Output: " {
            in_output = true;
            continue;
        }
        if in_output {
            output_lines.push(line);
        }
    }

    let mut result = String::new();
    if let Some(cmd) = cmd_line {
        result.push_str(cmd);
    }
    if !output_lines.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&output_lines.join("\n"));
    }

    if result.is_empty() {
        text.to_string()
    } else {
        result
    }
}

fn value_to_preview(value: Option<&serde_json::Value>) -> Option<String> {
    let v = value?;
    if v.is_null() {
        return None;
    }
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    serde_json::to_string(v).ok()
}

fn is_failed_status(status: &str) -> bool {
    let status = status.trim();
    status.eq_ignore_ascii_case("error")
        || status.eq_ignore_ascii_case("failed")
        || status.eq_ignore_ascii_case("failure")
        || status.eq_ignore_ascii_case("cancelled")
        || status.eq_ignore_ascii_case("canceled")
}

fn parse_nonzero_exit_code_from_line(line: &str) -> Option<i64> {
    let trimmed = line.trim();
    let (label, rest) = trimmed.split_once(':')?;
    if !label.trim_end().eq_ignore_ascii_case("exit code") {
        return None;
    }
    let number_text = rest.split_whitespace().next()?;
    let code = number_text.parse::<i64>().ok()?;
    if code == 0 {
        None
    } else {
        Some(code)
    }
}

fn infer_output_text_is_error(text: &str) -> bool {
    for line in text.lines().take(16) {
        if parse_nonzero_exit_code_from_line(line).is_some() {
            return true;
        }
    }

    for line in text.lines().take(32) {
        let lower = line.trim().to_ascii_lowercase();
        let shell_prefix =
            lower.starts_with("bash:") || lower.starts_with("zsh:") || lower.starts_with("sh:");
        if shell_prefix
            && (lower.contains("command not found")
                || lower.contains("no such file or directory")
                || lower.contains("permission denied"))
        {
            return true;
        }
    }

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(trimmed)
            .ok()
            .map(|v| infer_output_value_is_error(&v, 0))
            .unwrap_or(false)
    {
        return true;
    }

    trimmed
        .get(..6)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("error:"))
}

fn infer_output_value_is_error(value: &serde_json::Value, depth: usize) -> bool {
    if depth > 4 {
        return false;
    }

    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => false,
        serde_json::Value::String(text) => infer_output_text_is_error(text),
        serde_json::Value::Array(items) => items
            .iter()
            .any(|item| infer_output_value_is_error(item, depth + 1)),
        serde_json::Value::Object(map) => {
            if map
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return true;
            }

            if map.get("ok").and_then(|v| v.as_bool()) == Some(false)
                || map.get("success").and_then(|v| v.as_bool()) == Some(false)
            {
                return true;
            }

            if let Some(status) = map.get("status").and_then(|v| v.as_str()) {
                if is_failed_status(status) {
                    return true;
                }
            }

            if let Some(exit_code) = map.get("exit_code").and_then(|v| v.as_i64()) {
                if exit_code != 0 {
                    return true;
                }
            }

            if let Some(stderr) = map.get("stderr").and_then(|v| v.as_str()) {
                if !stderr.trim().is_empty() {
                    return true;
                }
            }

            if let Some(error) = map.get("error") {
                match error {
                    serde_json::Value::Null => {}
                    serde_json::Value::Bool(false) => {}
                    serde_json::Value::String(s) if s.trim().is_empty() => {}
                    _ => return true,
                }
            }

            for key in ["output", "result", "details", "data"] {
                if let Some(child) = map.get(key) {
                    if infer_output_value_is_error(child, depth + 1) {
                        return true;
                    }
                }
            }

            false
        }
    }
}

fn infer_tool_call_output_is_error(
    payload: &serde_json::Value,
    output_value: Option<&serde_json::Value>,
    output_preview: Option<&str>,
) -> bool {
    if payload
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return true;
    }

    if let Some(status) = payload.get("status").and_then(|s| s.as_str()) {
        if is_failed_status(status) {
            return true;
        }
    }

    if let Some(error) = payload.get("error") {
        match error {
            serde_json::Value::Null => {}
            serde_json::Value::Bool(false) => {}
            serde_json::Value::String(s) if s.trim().is_empty() => {}
            _ => return true,
        }
    }

    if let Some(output) = output_value {
        if infer_output_value_is_error(output, 0) {
            return true;
        }
    }

    output_preview
        .map(infer_output_text_is_error)
        .unwrap_or(false)
}

/// Synthetic rawInput key the live input shaper uses to carry the collab op
/// through to the card (see frontend `collab-tool.ts` `COLLAB_OP_KEY`). Kept in
/// sync here so history `wait_agent` capsules render with an op-aware title.
const COLLAB_OP_KEY: &str = "__iyw_claw_collab_op";

/// Whether a collab status string is an error (mirrors the frontend
/// `isErrorCollabStatusKind`: only `errored` / `failed` / `notFound`).
fn is_error_collab_status(status: &str) -> bool {
    matches!(status, "errored" | "failed" | "notFound")
}

/// Add `agent_id` to a spawn execution capsule's input JSON (the
/// `{subagent_type,prompt,description}` object), so the card can show the
/// sub-agent UUID. Tolerates a missing/!object input by starting fresh.
fn inject_agent_id_into_input(input: Option<&str>, agent_id: &str) -> String {
    let mut obj = input
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    obj.insert(
        "agent_id".to_string(),
        serde_json::Value::String(agent_id.to_string()),
    );
    serde_json::Value::Object(obj).to_string()
}

/// Pull a single sub-agent's `(status, message)` out of one `wait_agent`
/// `output.status` value, e.g. `{ "completed": "<result>" }`. Generalizes over
/// the terminal key: prefer `completed`, else the first string-valued key, so a
/// future `{ "errored": "<msg>" }` maps to `status="errored"`.
fn extract_wait_agent_status(value: &serde_json::Value) -> (String, Option<String>) {
    if let Some(obj) = value.as_object() {
        if let Some(text) = obj.get("completed").and_then(|v| v.as_str()) {
            return ("completed".to_string(), Some(text.to_string()));
        }
        for (key, val) in obj {
            if let Some(text) = val.as_str() {
                return (key.clone(), Some(text.to_string()));
            }
        }
    } else if let Some(text) = value.as_str() {
        return (text.to_string(), None);
    }
    ("completed".to_string(), None)
}

/// Build a synthesized live-shaped collab `rawInput` JSON (and whether any agent
/// errored) for a history `wait_agent` capsule, from that wait's own
/// `output.status` map `{ agent_id: { <terminal-key>: <text> } }`. The result
/// routes through the same `CollabAgentCard` as the live `wait` capsule (matches
/// the shape `collab-tool.ts` `parseCollabToolInput` expects). Caller guarantees
/// `status` is non-empty.
fn build_collab_wait_input(status: &serde_json::Map<String, serde_json::Value>) -> (String, bool) {
    let mut receiver_ids: Vec<serde_json::Value> = Vec::new();
    let mut agents_states = serde_json::Map::new();
    let mut any_error = false;
    for (agent_id, value) in status {
        receiver_ids.push(serde_json::Value::String(agent_id.clone()));
        let (st, msg) = extract_wait_agent_status(value);
        if is_error_collab_status(&st) {
            any_error = true;
        }
        agents_states.insert(
            agent_id.clone(),
            serde_json::json!({
                "status": st,
                "message": msg,
            }),
        );
    }
    let input = serde_json::json!({
        "senderThreadId": "",
        "receiverThreadIds": receiver_ids,
        "agentsStates": serde_json::Value::Object(agents_states),
        "status": if any_error { "failed" } else { "completed" },
        COLLAB_OP_KEY: "wait",
    });
    (input.to_string(), any_error)
}

fn parse_codex_subagent_stats(
    session_dir: &std::path::Path,
    agent_id: &str,
) -> Option<AgentExecutionStats> {
    if agent_id.len() > 64 || agent_id.contains("..") || agent_id.contains('/') {
        return None;
    }

    // Try exact filename first (e.g., "agent-{agent_id}.jsonl"), then fall
    // back to files whose stem ends with the agent_id. Collect and sort
    // candidates to ensure deterministic selection across platforms.
    let exact_path = session_dir.join(format!("agent-{}.jsonl", agent_id));
    let session_file = if exact_path.is_file() {
        exact_path
    } else {
        let mut candidates: Vec<_> = fs::read_dir(session_dir)
            .ok()?
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    return None;
                }
                let stem = path.file_stem()?.to_string_lossy().into_owned();
                // Match only if the stem ends with the agent_id after a separator
                // (e.g., "session-abc123" matches agent_id "abc123")
                if stem == agent_id
                    || stem
                        .strip_suffix(agent_id)
                        .is_some_and(|prefix| prefix.ends_with('-') || prefix.ends_with('_'))
                {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();
        candidates.sort();
        candidates.into_iter().next()?
    };

    let file = fs::File::open(&session_file).ok()?;
    let reader = BufReader::new(file);

    let mut tool_calls = Vec::new();
    let mut pending_calls: HashMap<String, AgentToolCall> = HashMap::new();
    let mut first_ts: Option<DateTime<Utc>> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;

    for line in reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(ts) = parse_codex_timestamp(&value) {
            if first_ts.is_none() {
                first_ts = Some(ts);
            }
            last_ts = Some(ts);
        }

        if value.get("type").and_then(|t| t.as_str()) != Some("response_item") {
            continue;
        }
        let payload = match value.get("payload") {
            Some(p) => p,
            None => continue,
        };
        let payload_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match payload_type {
            "function_call" | "custom_tool_call" => {
                let call_id = payload
                    .get("call_id")
                    .or_else(|| payload.get("tool_call_id"))
                    .or_else(|| payload.get("id"))
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_string());
                let tool_name = payload
                    .get("name")
                    .or_else(|| payload.get("tool_name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let input_preview = if tool_name == "exec_command" {
                    parse_codex_json_arg(payload)
                        .and_then(|a| a.get("cmd").and_then(|v| v.as_str()).map(|s| s.to_string()))
                        .or_else(|| {
                            value_to_preview(
                                payload.get("arguments").or_else(|| payload.get("input")),
                            )
                        })
                } else {
                    value_to_preview(payload.get("arguments").or_else(|| payload.get("input")))
                };

                let tc = AgentToolCall {
                    tool_name,
                    input_preview: input_preview.map(|s| truncate_str(&s, 500)),
                    output_preview: None,
                    is_error: false,
                };
                if let Some(id) = call_id {
                    pending_calls.insert(id, tc);
                } else {
                    tool_calls.push(tc);
                }
            }
            "function_call_output" | "custom_tool_call_output" => {
                let call_id = payload
                    .get("call_id")
                    .or_else(|| payload.get("tool_call_id"))
                    .or_else(|| payload.get("id"))
                    .and_then(|id| id.as_str());

                if let Some(id) = call_id {
                    if let Some(mut tc) = pending_calls.remove(id) {
                        let output_value = payload.get("output");
                        let raw_output = value_to_preview(output_value);
                        if tc.tool_name == "exec_command" {
                            tc.output_preview =
                                raw_output.map(|s| truncate_str(&clean_codex_exec_output(&s), 500));
                        } else {
                            tc.output_preview = raw_output.map(|s| truncate_str(&s, 500));
                        }
                        tc.is_error = infer_tool_call_output_is_error(
                            payload,
                            output_value,
                            tc.output_preview.as_deref(),
                        );
                        tool_calls.push(tc);
                    }
                }
            }
            _ => {}
        }
    }

    tool_calls.extend(pending_calls.into_values());

    let total_duration_ms = match (first_ts, last_ts) {
        (Some(f), Some(l)) => {
            let dur = (l - f).num_milliseconds();
            if dur > 0 {
                Some(dur as u64)
            } else {
                None
            }
        }
        _ => None,
    };

    let tool_count = tool_calls.len() as u32;
    Some(AgentExecutionStats {
        agent_type: None,
        status: None,
        total_duration_ms,
        total_tokens: None,
        total_tool_use_count: if tool_count > 0 {
            Some(tool_count)
        } else {
            None
        },
        read_count: None,
        search_count: None,
        bash_count: None,
        edit_file_count: None,
        lines_added: None,
        lines_removed: None,
        other_tool_count: None,
        tool_calls,
    })
}

impl CodexParser {
    fn parse_conversation_detail(
        &self,
        path: &PathBuf,
        conversation_id: &str,
    ) -> Result<ConversationDetail, ParseError> {
        let file = fs::File::open(path)?;
        let reader = BufReader::new(file);

        let mut messages = Vec::new();
        let mut cwd: Option<String> = None;
        let mut git_branch: Option<String> = None;
        let mut model: Option<String> = None;
        let mut title: Option<String> = None;
        // Objective of the goal run currently open while replaying `/goal`
        // transitions, so a persisted `thread_goal_updated` with `goal: null`
        // closes the run by objective — identical to the live path. See
        // `crate::acp::codex_goal::next_goal_marker`.
        let mut codex_open_goal: Option<String> = None;
        // Objective of the FIRST goal opened, captured for a post-parse fallback:
        // newer codex consumes `/goal <objective>` as a slash command (persists
        // `thread_goal_updated` but no `user_message`), so the typed `/goal …`
        // prompt has no user turn on reload. We surface the objective as the
        // leading user message AFTER the loop — never mid-loop — so the synthetic
        // message can never poison `should_skip_duplicate_user_message` and drop a
        // real same-text user message that arrives later in the file.
        let mut first_goal_objective: Option<String> = None;
        // Whether that first `/goal` OPENED the session — i.e. no real user turn
        // had been recorded when it arrived. This is positional, not "no user turn
        // anywhere": newer codex records the goal first with no `user_message`, so
        // the objective IS the opening prompt and must be surfaced as the leading
        // user turn even when a LATER real reply (e.g. a "确认") exists. Older codex
        // persisted the `/goal` text as the opening `user_message`, which arrives
        // BEFORE the goal — there the flag stays false and nothing is synthesized.
        let mut goal_opens_session = false;
        let mut last_turn_context_ts: Option<DateTime<Utc>> = None;
        let mut turn_model_contexts: Vec<(DateTime<Utc>, String)> = Vec::new();
        let mut context_window_used_tokens: Option<u64> = None;
        let mut context_window_max_tokens: Option<u64> = None;
        let mut latest_total_usage: Option<TurnUsage> = None;
        let mut latest_total_tokens: Option<u64> = None;

        let mut first_timestamp: Option<DateTime<Utc>> = None;
        let mut last_timestamp: Option<DateTime<Utc>> = None;

        // Agent subagent tracking (spawn_agent / wait_agent / close_agent).
        //
        // Capsule model (mirrors the live frontend, see collab-tool.ts):
        //   - spawn_agent → an "Agent" execution capsule (this file + nested
        //     stats from `agent-<id>.jsonl`). Shows the task + process; it does
        //     NOT carry the final result text (that lives in the wait capsule).
        //   - wait_agent  → a synthesized `collab_agent` capsule per wait, built
        //     from THAT wait's own `output.status` (the agents it returned). The
        //     full result text is shown here, via the same `CollabAgentCard` the
        //     live `wait` capsule uses. codex returns each agent's result in
        //     exactly one wait, so wait capsules never overlap.
        //   - close_agent → folded into the execution capsule (no own capsule);
        //     its result is only a fallback for agents never waited on.
        // codex-acp 1.0.1 (#223) maps `collabAgentToolCall` onto live ACP
        // `tool_call`s and still drops `subAgentActivity`, so the nested
        // `agent-<id>.jsonl` stats only exist on history reload. Live and
        // reconstructed capsules never double-render (live during streaming,
        // this on reload).
        let mut spawn_agent_call_ids: HashSet<String> = HashSet::new();
        let mut agent_id_to_spawn_call_id: HashMap<String, String> = HashMap::new();
        // Result text used to FILL the execution capsule only as a fallback for
        // agents that were never returned by a wait (keyed by agent_id). Filled
        // from close_agent's `previous_status`.
        let mut agent_fallback_results: HashMap<String, String> = HashMap::new();
        // Agents whose result was already shown in a wait capsule — their
        // execution capsule must NOT also show the result (no duplication).
        let mut agent_waited: HashSet<String> = HashSet::new();
        // Agents that ended in an error state (see `is_error_collab_status`:
        // errored/failed/notFound) in any wait or close — used to mark the
        // execution capsule as failed (live parity).
        let mut agent_errored: HashSet<String> = HashSet::new();
        let mut wait_agent_call_ids: HashSet<String> = HashSet::new();
        let mut close_agent_call_ids: HashSet<String> = HashSet::new();
        let mut close_agent_targets: HashMap<String, String> = HashMap::new();
        let mut active_agent_count: u32 = 0;
        let mut call_id_tool_names: HashMap<String, String> = HashMap::new();
        // Codex 0.129+ writes a generated image both as `event_msg.image_generation_end`
        // and as `response_item.image_generation_call`, sharing the same call_id/id.
        // Emit at most one ContentBlock::Image per id to avoid duplicate display.
        let mut emitted_image_ids: HashSet<String> = HashSet::new();
        // Streaming reasoning buffer. Codex emits one `event_msg.agent_reasoning`
        // per reasoning section, then groups the same sections into a single
        // `response_item.reasoning.summary`. We buffer the per-section events and
        // let the grouped summary supersede them (one 思考 card per turn, live
        // parity); the buffer is only flushed on its own — as one joined Thinking
        // block — when no grouped summary arrives (interrupted/older rollouts),
        // so streaming reasoning is never lost. `pending_reasoning_ts` stamps the
        // fallback block with the last buffered section's time.
        let mut pending_reasoning: Vec<String> = Vec::new();
        let mut pending_reasoning_ts: Option<DateTime<Utc>> = None;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.trim().is_empty() {
                continue;
            }

            let value: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let msg_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");

            if let Some(ts_str) = value.get("timestamp").and_then(|t| t.as_str()) {
                if let Ok(ts) = ts_str.parse::<DateTime<Utc>>() {
                    if first_timestamp.is_none() {
                        first_timestamp = Some(ts);
                    }
                    last_timestamp = Some(ts);
                }
            }

            match msg_type {
                "session_meta" => {
                    if let Some(payload) = value.get("payload") {
                        cwd = payload
                            .get("cwd")
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string());
                        git_branch = payload
                            .get("git")
                            .and_then(|g| g.get("branch"))
                            .and_then(|b| b.as_str())
                            .map(|s| s.to_string());
                    }
                }
                "turn_context" => {
                    // A new API turn means any prior agent lifecycle is complete.
                    active_agent_count = 0;
                    let turn_model = value
                        .get("payload")
                        .and_then(|p| p.get("model"))
                        .and_then(|m| m.as_str())
                        .map(str::trim)
                        .filter(|model| !model.is_empty());
                    if model.is_none() {
                        model = turn_model.map(str::to_owned);
                    }
                    last_turn_context_ts = parse_codex_timestamp(&value);
                    if let (Some(timestamp), Some(turn_model)) = (last_turn_context_ts, turn_model)
                    {
                        turn_model_contexts.push((timestamp, turn_model.to_owned()));
                    }
                }
                "event_msg" => {
                    if let Some(payload) = value.get("payload") {
                        let payload_type =
                            payload.get("type").and_then(|t| t.as_str()).unwrap_or("");

                        let timestamp = parse_codex_timestamp(&value).unwrap_or_else(Utc::now);

                        // A new reasoning section keeps buffering; `token_count` is
                        // metadata with no visible message and never splits a run.
                        // Anything else closes an open reasoning run — flush any
                        // buffered streaming reasoning that never got a grouped
                        // summary so it isn't lost or reordered behind this event.
                        if payload_type != "agent_reasoning" && payload_type != "token_count" {
                            flush_pending_reasoning(
                                &mut messages,
                                &mut pending_reasoning,
                                pending_reasoning_ts,
                            );
                        }

                        match payload_type {
                            "task_started" if context_window_max_tokens.is_none() => {
                                context_window_max_tokens =
                                    payload.get("model_context_window").and_then(|v| v.as_u64());
                            }
                            "user_message" => {
                                active_agent_count = 0;
                                let text = payload
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let normalized = strip_blocked_resource_mentions(&text);
                                let mut blocks: Vec<ContentBlock> = Vec::new();
                                if !normalized.is_empty() {
                                    blocks.push(ContentBlock::Text { text: normalized });
                                }

                                if let Some(images) =
                                    payload.get("images").and_then(|v| v.as_array())
                                {
                                    for image in images {
                                        let Some(raw) = image.as_str() else {
                                            continue;
                                        };
                                        if let Some(image) = parse_image_reference(raw) {
                                            blocks.push(image);
                                        }
                                    }
                                }

                                if blocks.is_empty() {
                                    blocks.push(ContentBlock::Text {
                                        text: "Attached resources".to_string(),
                                    });
                                }

                                if title.is_none() {
                                    title = extract_codex_title_candidate(&text, true);
                                }

                                if should_skip_duplicate_user_message(&messages, &blocks, timestamp)
                                {
                                    continue;
                                }

                                messages.push(UnifiedMessage {
                                    id: format!("user-{}", messages.len()),
                                    role: MessageRole::User,
                                    content: blocks,
                                    timestamp,
                                    usage: None,
                                    duration_ms: None,
                                    model: None,
                                    completed_at: Some(timestamp),
                                });
                            }
                            "agent_message" => {
                                // Parent narration is emitted even while a
                                // sub-agent is active (active_agent_count > 0).
                                // codex-acp 1.0.x writes the sub-agent's own
                                // transcript to its `agent-<id>.jsonl`, NOT into
                                // the parent rollout, so every agent_message here
                                // is the parent's (verified across 180 real
                                // rollouts: 0 sub-agent leaks). The old
                                // `active_agent_count == 0` guard wrongly dropped
                                // the parent's between-capsule narration — and,
                                // when no close_agent ran (active never returns to
                                // 0), even the final answer. Images keep their own
                                // guard (see image_generation arms).
                                let text = payload
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                messages.push(UnifiedMessage {
                                    id: format!("assistant-{}", messages.len()),
                                    role: MessageRole::Assistant,
                                    content: vec![ContentBlock::Text { text }],
                                    timestamp,
                                    usage: None,
                                    duration_ms: None,
                                    model: None,
                                    completed_at: Some(timestamp),
                                });
                            }
                            "thread_goal_updated" => {
                                // codex-acp v1.1.0 (#263) routes live goals through
                                // `session_info_update`; the CLI has always persisted
                                // each `/goal` transition to the rollout as
                                // `event_msg.thread_goal_updated.goal`. Synthesize the
                                // same canonical create_goal/update_goal
                                // tool_use+tool_result the live path emits (shared
                                // mapping in `crate::acp::codex_goal`) so a reloaded
                                // conversation renders goal cards identical to live —
                                // history that never surfaced goals before.
                                if let Some(marker) = payload.get("goal").and_then(|goal| {
                                    crate::acp::codex_goal::next_goal_marker(
                                        &mut codex_open_goal,
                                        goal,
                                    )
                                }) {
                                    // Remember the first opened goal's objective for
                                    // the post-parse leading-user-message fallback (see
                                    // `first_goal_objective`). Deferred, not synthesized
                                    // here, so it can't interfere with duplicate
                                    // suppression of a later real user message.
                                    if marker.tool_name == "create_goal"
                                        && first_goal_objective.is_none()
                                    {
                                        // Positional: the goal opened the session iff no
                                        // real user turn exists yet.
                                        goal_opens_session = !messages
                                            .iter()
                                            .any(|m| matches!(m.role, MessageRole::User));
                                        // Claim the title from the objective HERE, in
                                        // stream order, when the goal is the opener — so
                                        // a LATER `user_message` (its own guard is
                                        // `title.is_none()`) can't steal it, while an
                                        // EARLIER real user (older codex) or a native
                                        // `thread_name_updated` still wins.
                                        if goal_opens_session && title.is_none() {
                                            title = extract_codex_title_candidate(
                                                &marker.objective,
                                                true,
                                            );
                                        }
                                        first_goal_objective = Some(marker.objective.clone());
                                    }
                                    // Occurrence id from the message index — unique
                                    // per goal event, stable across reparse, and
                                    // shared by this event's ToolUse + ToolResult.
                                    let id = crate::acp::codex_goal::goal_tool_call_id(
                                        messages.len() as u64,
                                    );
                                    messages.push(UnifiedMessage {
                                        id: format!("tool-{}", messages.len()),
                                        role: MessageRole::Assistant,
                                        content: vec![
                                            ContentBlock::ToolUse {
                                                tool_use_id: Some(id.clone()),
                                                tool_name: marker.tool_name.to_string(),
                                                input_preview: Some(marker.input_json),
                                                meta: None,
                                            },
                                            ContentBlock::ToolResult {
                                                tool_use_id: Some(id),
                                                output_preview: Some(marker.output_json),
                                                is_error: false,
                                                agent_stats: None,
                                                images: Vec::new(),
                                            },
                                        ],
                                        timestamp,
                                        usage: None,
                                        duration_ms: None,
                                        model: None,
                                        completed_at: Some(timestamp),
                                    });
                                }
                            }
                            "thread_name_updated" => {
                                // Codex's native thread name — adopt it as the
                                // auto-title (parity with Claude `aiTitle`, Gemini
                                // `update_topic`, OpenCode `session.title`). Newest
                                // non-empty wins, overriding the first-prompt
                                // fallback; the title coordinator's lock guard
                                // guard still respects a manual rename.
                                // Rollout persists `thread_name` (snake_case); the
                                // live ACP notification uses `threadName`. Accept
                                // both so the parser is robust to either source.
                                if let Some(name) = payload
                                    .get("thread_name")
                                    .or_else(|| payload.get("threadName"))
                                    .or_else(|| payload.get("name"))
                                    .and_then(|n| n.as_str())
                                    .map(str::trim)
                                    .filter(|n| !n.is_empty())
                                {
                                    title = Some(truncate_str(name, 100));
                                }
                            }
                            "agent_reasoning" => {
                                // Buffer this streaming reasoning section. The grouped
                                // `response_item.reasoning.summary` (parsed in the
                                // `response_item` match below) normally arrives right
                                // after the section events and supersedes the buffer,
                                // so history shows ONE 思考 card per turn (live parity)
                                // instead of one card per section. If no grouped
                                // summary arrives (interrupted/older rollouts), the
                                // buffer is flushed on its own and nothing is lost.
                                let text =
                                    payload.get("text").and_then(|t| t.as_str()).unwrap_or("");
                                if !text.trim().is_empty() {
                                    pending_reasoning.push(text.to_string());
                                    pending_reasoning_ts = Some(timestamp);
                                }
                            }
                            "image_generation_end" => {
                                if active_agent_count > 0 {
                                    continue;
                                }
                                let call_id = payload
                                    .get("call_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let result =
                                    payload.get("result").and_then(|v| v.as_str()).unwrap_or("");
                                if result.is_empty() {
                                    continue;
                                }
                                if !call_id.is_empty() && emitted_image_ids.contains(&call_id) {
                                    continue;
                                }
                                let mime_type = payload
                                    .get("mime_type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("image/png")
                                    .to_string();
                                let uri = payload
                                    .get("saved_path")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                                let revised_prompt = payload
                                    .get("revised_prompt")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .filter(|s| !s.trim().is_empty());
                                messages.push(UnifiedMessage {
                                    id: format!("assistant-imagegen-{}", messages.len()),
                                    role: MessageRole::Assistant,
                                    content: vec![ContentBlock::ImageGeneration {
                                        revised_prompt,
                                        image: Some(ImageData {
                                            data: result.to_string(),
                                            mime_type,
                                            uri,
                                        }),
                                    }],
                                    timestamp,
                                    usage: None,
                                    duration_ms: None,
                                    model: None,
                                    completed_at: Some(timestamp),
                                });
                                if !call_id.is_empty() {
                                    emitted_image_ids.insert(call_id);
                                }
                            }
                            "token_count" => {
                                if let Some(info) = payload.get("info") {
                                    if let Some(total_usage_payload) = info.get("total_token_usage")
                                    {
                                        if let Some(total_usage) =
                                            extract_turn_usage_from_codex_usage(total_usage_payload)
                                        {
                                            latest_total_usage = Some(total_usage);
                                        }
                                        if let Some(total_tokens) =
                                            extract_total_tokens_from_usage(total_usage_payload)
                                        {
                                            latest_total_tokens = Some(total_tokens);
                                        }
                                    }

                                    let total_tokens =
                                        extract_context_window_used_tokens_from_token_count_info(
                                            info,
                                        );
                                    if total_tokens.is_some() {
                                        context_window_used_tokens = total_tokens;
                                    }

                                    let context_window =
                                        info.get("model_context_window").and_then(|v| v.as_u64());
                                    if context_window.is_some() {
                                        context_window_max_tokens = context_window;
                                    }

                                    if !info.is_null() {
                                        if let Some(usage) = info
                                            .get("last_token_usage")
                                            .and_then(extract_turn_usage_from_codex_usage)
                                        {
                                            // Attach to the last assistant message
                                            if let Some(last_msg) = messages
                                                .iter_mut()
                                                .rev()
                                                .find(|m| matches!(m.role, MessageRole::Assistant))
                                            {
                                                if last_msg.usage.is_none() {
                                                    last_msg.usage = Some(usage);
                                                }
                                            }
                                        }
                                    }
                                }
                                // Compute duration from turn_context to token_count
                                if let (Some(start_ts), Some(end_ts)) =
                                    (last_turn_context_ts, parse_codex_timestamp(&value))
                                {
                                    let duration = (end_ts - start_ts).num_milliseconds();
                                    if duration > 0 {
                                        if let Some(last_msg) = messages
                                            .iter_mut()
                                            .rev()
                                            .find(|m| matches!(m.role, MessageRole::Assistant))
                                        {
                                            if last_msg.duration_ms.is_none() {
                                                last_msg.duration_ms = Some(duration as u64);
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "response_item" => {
                    if let Some(payload) = value.get("payload") {
                        let payload_type =
                            payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        let timestamp = parse_codex_timestamp(&value).unwrap_or_else(Utc::now);

                        // A `reasoning` item resolves the buffered streaming sections
                        // (handled in its arm). Any other response item closes an open
                        // reasoning run — flush buffered streaming reasoning that never
                        // got a grouped summary so it isn't lost or reordered.
                        if payload_type != "reasoning" {
                            flush_pending_reasoning(
                                &mut messages,
                                &mut pending_reasoning,
                                pending_reasoning_ts,
                            );
                        }

                        match payload_type {
                            "reasoning" => {
                                // Codex records a reasoning turn as a `summary` array
                                // of `{type:"summary_text", text}` parts — one part per
                                // section — grouping the same sections the streaming
                                // `event_msg.agent_reasoning` events carry one-by-one
                                // (buffered in `pending_reasoning`). Join the parts into
                                // ONE Thinking block (live parity: a single 思考 card
                                // per turn) and discard the buffer it supersedes. An
                                // empty summary (encrypted-only reasoning, the common
                                // case) carries no surfaced text, so fall back to any
                                // buffered streaming sections (interrupted/older
                                // rollouts) and otherwise emit nothing.
                                let text = payload
                                    .get("summary")
                                    .and_then(|s| s.as_array())
                                    .map(|parts| {
                                        parts
                                            .iter()
                                            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                                            .filter(|t| !t.trim().is_empty())
                                            .collect::<Vec<_>>()
                                            .join("\n\n")
                                    })
                                    .unwrap_or_default();
                                if !text.is_empty() {
                                    pending_reasoning.clear();
                                    messages.push(UnifiedMessage {
                                        id: format!("thinking-{}", messages.len()),
                                        role: MessageRole::Assistant,
                                        content: vec![ContentBlock::Thinking { text }],
                                        timestamp,
                                        usage: None,
                                        duration_ms: None,
                                        model: None,
                                        completed_at: Some(timestamp),
                                    });
                                } else {
                                    flush_pending_reasoning(
                                        &mut messages,
                                        &mut pending_reasoning,
                                        pending_reasoning_ts,
                                    );
                                }
                            }
                            "function_call" | "custom_tool_call" => {
                                let tool_use_id = payload
                                    .get("call_id")
                                    .or_else(|| payload.get("tool_call_id"))
                                    .or_else(|| payload.get("id"))
                                    .and_then(|id| id.as_str())
                                    .map(|s| s.to_string());
                                let raw_tool_name = payload
                                    .get("name")
                                    .or_else(|| payload.get("tool_name"))
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("unknown");

                                match raw_tool_name {
                                    "spawn_agent" => {
                                        let args = parse_codex_json_arg(payload);
                                        let agent_type = args
                                            .as_ref()
                                            .and_then(|a| a.get("agent_type"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("agent");
                                        let message = args
                                            .as_ref()
                                            .and_then(|a| a.get("message"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let description =
                                            truncate_str(message.lines().next().unwrap_or(""), 60);

                                        if let Some(ref id) = tool_use_id {
                                            spawn_agent_call_ids.insert(id.clone());
                                        }
                                        active_agent_count += 1;

                                        let agent_input = serde_json::json!({
                                            "subagent_type": agent_type,
                                            "prompt": message,
                                            "description": description,
                                        });

                                        messages.push(UnifiedMessage {
                                            id: format!("tool-{}", messages.len()),
                                            role: MessageRole::Assistant,
                                            content: vec![ContentBlock::ToolUse {
                                                tool_use_id,
                                                tool_name: "Agent".to_string(),
                                                input_preview: Some(agent_input.to_string()),
                                                meta: None,
                                            }],
                                            timestamp,
                                            usage: None,
                                            duration_ms: None,
                                            model: None,
                                            completed_at: Some(timestamp),
                                        });
                                    }
                                    "wait_agent" => {
                                        if let Some(ref id) = tool_use_id {
                                            wait_agent_call_ids.insert(id.clone());
                                        }
                                    }
                                    "close_agent" => {
                                        if let Some(ref id) = tool_use_id {
                                            close_agent_call_ids.insert(id.clone());
                                            let target =
                                                parse_codex_json_arg(payload).and_then(|a| {
                                                    a.get("target")
                                                        .and_then(|v| v.as_str())
                                                        .map(|s| s.to_string())
                                                });
                                            if let Some(target) = target {
                                                close_agent_targets.insert(id.clone(), target);
                                            }
                                        }
                                    }
                                    _ => {
                                        if let Some(ref id) = tool_use_id {
                                            call_id_tool_names
                                                .insert(id.clone(), raw_tool_name.to_string());
                                        }
                                        let input_preview = if raw_tool_name == "exec_command" {
                                            parse_codex_json_arg(payload)
                                                .and_then(|a| {
                                                    a.get("cmd")
                                                        .and_then(|v| v.as_str())
                                                        .map(|s| s.to_string())
                                                })
                                                .or_else(|| {
                                                    value_to_preview(
                                                        payload
                                                            .get("arguments")
                                                            .or_else(|| payload.get("input")),
                                                    )
                                                })
                                        } else {
                                            value_to_preview(
                                                payload
                                                    .get("arguments")
                                                    .or_else(|| payload.get("input")),
                                            )
                                        };
                                        messages.push(UnifiedMessage {
                                            id: format!("tool-{}", messages.len()),
                                            role: MessageRole::Assistant,
                                            content: vec![ContentBlock::ToolUse {
                                                tool_use_id,
                                                tool_name: raw_tool_name.to_string(),
                                                input_preview,
                                                meta: None,
                                            }],
                                            timestamp,
                                            usage: None,
                                            duration_ms: None,
                                            model: None,
                                            completed_at: Some(timestamp),
                                        });
                                    }
                                }
                            }
                            "function_call_output" | "custom_tool_call_output" => {
                                let tool_use_id = payload
                                    .get("call_id")
                                    .or_else(|| payload.get("tool_call_id"))
                                    .or_else(|| payload.get("id"))
                                    .and_then(|id| id.as_str())
                                    .map(|s| s.to_string());

                                let is_spawn = tool_use_id
                                    .as_ref()
                                    .is_some_and(|id| spawn_agent_call_ids.contains(id));
                                let is_wait = tool_use_id
                                    .as_ref()
                                    .is_some_and(|id| wait_agent_call_ids.contains(id));
                                let is_close = tool_use_id
                                    .as_ref()
                                    .is_some_and(|id| close_agent_call_ids.contains(id));

                                if is_spawn {
                                    if let Some(output_obj) = parse_codex_json_output(payload) {
                                        if let (Some(agent_id), Some(call_id)) = (
                                            output_obj.get("agent_id").and_then(|v| v.as_str()),
                                            tool_use_id.as_ref(),
                                        ) {
                                            agent_id_to_spawn_call_id
                                                .insert(agent_id.to_string(), call_id.clone());
                                        }
                                    }
                                    messages.push(UnifiedMessage {
                                        id: format!("tool-result-{}", messages.len()),
                                        role: MessageRole::Tool,
                                        content: vec![ContentBlock::ToolResult {
                                            tool_use_id,
                                            output_preview: None,
                                            is_error: false,
                                            agent_stats: None,
                                            images: Vec::new(),
                                        }],
                                        timestamp,
                                        usage: None,
                                        duration_ms: None,
                                        model: None,
                                        completed_at: Some(timestamp),
                                    });
                                } else if is_wait {
                                    // Emit one `collab_agent` capsule per wait,
                                    // built from THIS wait's own returned agents
                                    // (`output.status`). Routes through the same
                                    // CollabAgentCard as the live wait capsule.
                                    if let Some(output_obj) = parse_codex_json_output(payload) {
                                        if let Some(status) =
                                            output_obj.get("status").and_then(|s| s.as_object())
                                        {
                                            // Mark returned agents so the spawn
                                            // capsule won't also show their result,
                                            // and record per-agent error state so
                                            // the execution capsule can render
                                            // failed (live parity).
                                            for (agent_id, value) in status {
                                                agent_waited.insert(agent_id.clone());
                                                let (st, _) = extract_wait_agent_status(value);
                                                if is_error_collab_status(&st) {
                                                    agent_errored.insert(agent_id.clone());
                                                }
                                            }
                                            if !status.is_empty() {
                                                let (collab_input, is_error) =
                                                    build_collab_wait_input(status);
                                                messages.push(UnifiedMessage {
                                                    id: format!("tool-{}", messages.len()),
                                                    role: MessageRole::Assistant,
                                                    content: vec![ContentBlock::ToolUse {
                                                        tool_use_id: tool_use_id.clone(),
                                                        tool_name: "collab_agent".to_string(),
                                                        input_preview: Some(collab_input),
                                                        meta: None,
                                                    }],
                                                    timestamp,
                                                    usage: None,
                                                    duration_ms: None,
                                                    model: None,
                                                    completed_at: Some(timestamp),
                                                });
                                                messages.push(UnifiedMessage {
                                                    id: format!("tool-result-{}", messages.len()),
                                                    role: MessageRole::Tool,
                                                    content: vec![ContentBlock::ToolResult {
                                                        tool_use_id,
                                                        output_preview: None,
                                                        is_error,
                                                        agent_stats: None,
                                                        images: Vec::new(),
                                                    }],
                                                    timestamp,
                                                    usage: None,
                                                    duration_ms: None,
                                                    model: None,
                                                    completed_at: Some(timestamp),
                                                });
                                            }
                                        }
                                    }
                                } else if is_close {
                                    active_agent_count = active_agent_count.saturating_sub(1);
                                    if let Some(output_obj) = parse_codex_json_output(payload) {
                                        if let Some(agent_id) = tool_use_id
                                            .as_ref()
                                            .and_then(|id| close_agent_targets.get(id))
                                        {
                                            // Generalize over the terminal key (not
                                            // just `completed`): an errored/notFound
                                            // close with no wait must not lose its
                                            // message or its error state.
                                            if let Some(prev) = output_obj.get("previous_status") {
                                                let (st, msg) = extract_wait_agent_status(prev);
                                                if let Some(text) = msg {
                                                    agent_fallback_results
                                                        .entry(agent_id.clone())
                                                        .or_insert(text);
                                                }
                                                if is_error_collab_status(&st) {
                                                    agent_errored.insert(agent_id.clone());
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    let is_exec = tool_use_id.as_ref().is_some_and(|id| {
                                        call_id_tool_names
                                            .get(id)
                                            .is_some_and(|n| n == "exec_command")
                                    });
                                    let output_value = payload.get("output");
                                    let raw_output = value_to_preview(output_value);
                                    let output = if is_exec {
                                        raw_output.map(|s| clean_codex_exec_output(&s))
                                    } else {
                                        raw_output
                                    };
                                    let is_error = infer_tool_call_output_is_error(
                                        payload,
                                        output_value,
                                        output.as_deref(),
                                    );
                                    messages.push(UnifiedMessage {
                                        id: format!("tool-result-{}", messages.len()),
                                        role: MessageRole::Tool,
                                        content: vec![ContentBlock::ToolResult {
                                            tool_use_id,
                                            output_preview: output,
                                            is_error,
                                            agent_stats: None,
                                            images: Vec::new(),
                                        }],
                                        timestamp,
                                        usage: None,
                                        duration_ms: None,
                                        model: None,
                                        completed_at: Some(timestamp),
                                    });
                                }
                            }
                            "message" => {
                                let role =
                                    payload.get("role").and_then(|r| r.as_str()).unwrap_or("");
                                if role == "user" {
                                    active_agent_count = 0;
                                    if let Some(blocks) =
                                        extract_response_item_user_image_blocks(payload)
                                    {
                                        if should_skip_duplicate_user_message(
                                            &messages, &blocks, timestamp,
                                        ) {
                                            continue;
                                        }

                                        if title.is_none() {
                                            if let Some(text) = first_text_block(&blocks) {
                                                title = extract_codex_title_candidate(
                                                    text.as_str(),
                                                    true,
                                                );
                                            }
                                        }

                                        messages.push(UnifiedMessage {
                                            id: format!("user-{}", messages.len()),
                                            role: MessageRole::User,
                                            content: blocks,
                                            timestamp,
                                            usage: None,
                                            duration_ms: None,
                                            model: None,
                                            completed_at: Some(timestamp),
                                        });
                                    }
                                }
                            }
                            "image_generation_call" => {
                                // codex 0.129+ writes the same generated image as both an
                                // `event_msg.image_generation_end` (earlier in the file) and
                                // a `response_item.image_generation_call` (here). They share
                                // the same id; emit at most once via emitted_image_ids.
                                // Subagent suppression mirrors the event_msg arm: parent
                                // timeline must not host children's generated images.
                                if active_agent_count > 0 {
                                    continue;
                                }
                                let id = payload
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                if !id.is_empty() && emitted_image_ids.contains(&id) {
                                    continue;
                                }
                                let result =
                                    payload.get("result").and_then(|v| v.as_str()).unwrap_or("");
                                if result.is_empty() {
                                    continue;
                                }
                                let mime_type = payload
                                    .get("mime_type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("image/png")
                                    .to_string();
                                let revised_prompt = payload
                                    .get("revised_prompt")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .filter(|s| !s.trim().is_empty());
                                messages.push(UnifiedMessage {
                                    id: format!("assistant-imagegen-{}", messages.len()),
                                    role: MessageRole::Assistant,
                                    content: vec![ContentBlock::ImageGeneration {
                                        revised_prompt,
                                        image: Some(ImageData {
                                            data: result.to_string(),
                                            mime_type,
                                            // response_item.image_generation_call has no
                                            // saved_path; event_msg.image_generation_end is
                                            // the only carrier of the on-disk file URI.
                                            uri: None,
                                        }),
                                    }],
                                    timestamp,
                                    usage: None,
                                    duration_ms: None,
                                    model: None,
                                    completed_at: Some(timestamp),
                                });
                                if !id.is_empty() {
                                    emitted_image_ids.insert(id);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        // Streaming reasoning at the very end of a truncated/interrupted rollout
        // (the `agent_reasoning` events were written but the file ended before the
        // grouped `response_item.reasoning` summary) — flush it so it isn't lost.
        flush_pending_reasoning(&mut messages, &mut pending_reasoning, pending_reasoning_ts);

        // Fill in subagent tool call stats (and, only as a fallback, the result)
        // on each spawn execution capsule.
        if !agent_id_to_spawn_call_id.is_empty() {
            let spawn_call_to_agent: HashMap<&str, &str> = agent_id_to_spawn_call_id
                .iter()
                .map(|(agent_id, call_id)| (call_id.as_str(), agent_id.as_str()))
                .collect();

            let session_dir = path.parent();
            let mut agent_stats_cache: HashMap<String, Option<AgentExecutionStats>> =
                HashMap::new();

            for msg in &mut messages {
                for block in &mut msg.content {
                    match block {
                        ContentBlock::ToolResult {
                            tool_use_id: Some(ref id),
                            ref mut output_preview,
                            ref mut is_error,
                            ref mut agent_stats,
                            ..
                        } => {
                            if let Some(&agent_id) = spawn_call_to_agent.get(id.as_str()) {
                                // The result text normally lives in the wait
                                // capsule; only show it on the execution capsule
                                // when this agent was never returned by a wait
                                // (else duplicate).
                                if !agent_waited.contains(agent_id) {
                                    if let Some(result) = agent_fallback_results.get(agent_id) {
                                        *output_preview = Some(result.clone());
                                    }
                                }
                                // Mark the execution capsule failed when the agent
                                // ended in error (in a wait or close) — live parity.
                                if agent_errored.contains(agent_id) {
                                    *is_error = true;
                                }
                                if let Some(dir) = session_dir {
                                    let stats = agent_stats_cache
                                        .entry(agent_id.to_string())
                                        .or_insert_with(|| {
                                            parse_codex_subagent_stats(dir, agent_id)
                                        });
                                    if stats.is_some() {
                                        *agent_stats = stats.clone();
                                    }
                                }
                            }
                        }
                        // Stamp the sub-agent's id onto the spawn execution capsule
                        // input so the card can render it (parity with the wait
                        // capsule, whose agentsStates already carry the id).
                        ContentBlock::ToolUse {
                            tool_use_id: Some(ref id),
                            ref tool_name,
                            ref mut input_preview,
                            ..
                        } if tool_name == "Agent" => {
                            if let Some(&agent_id) = spawn_call_to_agent.get(id.as_str()) {
                                *input_preview = Some(inject_agent_id_into_input(
                                    input_preview.as_deref(),
                                    agent_id,
                                ));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Leading-`/goal` fallback: when a `/goal` opened the session (before any
        // real user turn), newer codex recorded only `thread_goal_updated` — no
        // `user_message` — so the typed `/goal <objective>` prompt would be missing
        // on reload (headless, or, when a later reply like "确认" exists, starting
        // mid-conversation). Surface it as the leading user message, prefixed with
        // `/goal ` to match what the user actually typed and the live optimistic
        // bubble. The title was already claimed in-loop (see the goal-capture
        // block). Applied here, after parsing, so the synthetic turn never
        // participates in `should_skip_duplicate_user_message`.
        if let Some(objective) = first_goal_objective {
            if goal_opens_session {
                messages.insert(
                    0,
                    UnifiedMessage {
                        id: "codex-goal-user".to_string(),
                        role: MessageRole::User,
                        content: vec![ContentBlock::Text {
                            text: format!("/goal {objective}"),
                        }],
                        // Earliest event time so it sorts ahead of the goal card.
                        timestamp: first_timestamp.unwrap_or_else(Utc::now),
                        usage: None,
                        duration_ms: None,
                        model: None,
                        completed_at: first_timestamp,
                    },
                );
            }
        }

        let folder_path = cwd.clone();
        let folder_name = folder_path.as_ref().map(|p| folder_name_from_path(p));

        let mut turns = group_into_turns(messages);
        assign_codex_turn_models(&mut turns, &turn_model_contexts);
        super::relocate_orphaned_tool_results(&mut turns);
        super::structurize_read_tool_output(&mut turns);
        super::resolve_patch_line_numbers(&mut turns, cwd.as_deref());
        let mut session_stats = super::compute_session_stats(&turns);
        session_stats =
            merge_codex_total_usage_stats(session_stats, latest_total_usage, latest_total_tokens);
        session_stats = merge_codex_context_window_stats(
            session_stats,
            context_window_used_tokens,
            context_window_max_tokens,
        );

        let summary = ConversationSummary {
            id: conversation_id.to_string(),
            agent_type: AgentType::Codex,
            folder_path,
            folder_name,
            title,
            started_at: first_timestamp.unwrap_or_else(Utc::now),
            ended_at: last_timestamp,
            message_count: turns.len() as u32,
            model,
            git_branch,
            parent_id: None,
            parent_tool_use_id: None,
            delegation_call_id: None,
        };

        Ok(ConversationDetail {
            summary,
            turns,
            session_stats,
            transcript_watermark: None,
        })
    }
}

fn assign_codex_turn_models(turns: &mut [MessageTurn], contexts: &[(DateTime<Utc>, String)]) {
    for turn in turns
        .iter_mut()
        .filter(|turn| matches!(turn.role, TurnRole::Assistant) && turn.model.is_none())
    {
        turn.model = contexts
            .iter()
            .rev()
            .find(|(timestamp, _)| *timestamp <= turn.timestamp)
            .map(|(_, model)| model.clone());
    }
}

fn extract_total_tokens_from_usage(usage: &serde_json::Value) -> Option<u64> {
    if let Some(total_tokens) = usage.get("total_tokens").and_then(|v| v.as_u64()) {
        return Some(total_tokens);
    }

    let input_tokens = usage
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cached_input_tokens = usage
        .get("cached_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let reasoning_output_tokens = usage
        .get("reasoning_output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Codex payloads use `input_tokens` as the full input (cache read included),
    // so fallback totals should not double-count cached tokens.
    let total = if cached_input_tokens <= input_tokens {
        input_tokens + output_tokens + reasoning_output_tokens
    } else {
        input_tokens + cached_input_tokens + output_tokens + reasoning_output_tokens
    };
    if total > 0 {
        Some(total)
    } else {
        None
    }
}

fn extract_turn_usage_from_codex_usage(usage: &serde_json::Value) -> Option<TurnUsage> {
    let input_tokens = usage
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_read_input_tokens = usage
        .get("cached_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if input_tokens == 0 && output_tokens == 0 && cache_read_input_tokens == 0 {
        return None;
    }

    Some(TurnUsage {
        input_tokens: input_tokens.saturating_sub(cache_read_input_tokens),
        output_tokens,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens,
    })
}

fn extract_context_window_used_tokens_from_token_count_info(
    info: &serde_json::Value,
) -> Option<u64> {
    // `last_token_usage` is the current turn usage and best matches context window occupancy.
    if let Some(last_usage) = info.get("last_token_usage") {
        if let Some(total) = extract_total_tokens_from_usage(last_usage) {
            return Some(total);
        }
    }

    // Fallback: some payloads may only have cumulative totals.
    info.get("total_token_usage")
        .and_then(extract_total_tokens_from_usage)
}

fn merge_codex_context_window_stats(
    stats: Option<SessionStats>,
    used_tokens: Option<u64>,
    max_tokens: Option<u64>,
) -> Option<SessionStats> {
    if used_tokens.is_none() && max_tokens.is_none() {
        return stats;
    }

    let usage_percent = match (used_tokens, max_tokens) {
        (Some(used), Some(max)) if max > 0 => Some((used as f64 / max as f64) * 100.0),
        _ => None,
    };

    match stats {
        Some(mut s) => {
            s.context_window_used_tokens = used_tokens;
            s.context_window_max_tokens = max_tokens;
            s.context_window_usage_percent = usage_percent;
            Some(s)
        }
        None => Some(SessionStats {
            total_usage: None,
            total_tokens: None,
            total_duration_ms: 0,
            context_window_used_tokens: used_tokens,
            context_window_max_tokens: max_tokens,
            context_window_usage_percent: usage_percent,
        }),
    }
}

fn merge_codex_total_usage_stats(
    stats: Option<SessionStats>,
    total_usage: Option<TurnUsage>,
    total_tokens: Option<u64>,
) -> Option<SessionStats> {
    match stats {
        Some(mut s) => {
            if let Some(total) = total_usage {
                s.total_usage = Some(total);
            }
            if total_tokens.is_some() {
                s.total_tokens = total_tokens;
            }
            Some(s)
        }
        None if total_usage.is_some() || total_tokens.is_some() => Some(SessionStats {
            total_usage,
            total_tokens,
            total_duration_ms: 0,
            context_window_used_tokens: None,
            context_window_max_tokens: None,
            context_window_usage_percent: None,
        }),
        None => None,
    }
}

fn parse_codex_timestamp(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    value
        .get("timestamp")
        .and_then(|t| t.as_str())
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
}

/// Emit any buffered streaming `agent_reasoning` sections as a single Thinking
/// message and clear the buffer. No-op when the buffer is empty. Used only as a
/// fallback when the grouped `response_item.reasoning.summary` (which normally
/// supersedes and clears the buffer) is absent — e.g. an interrupted rollout —
/// so streaming reasoning is preserved as one 思考 card instead of being lost.
fn flush_pending_reasoning(
    messages: &mut Vec<UnifiedMessage>,
    pending: &mut Vec<String>,
    ts: Option<DateTime<Utc>>,
) {
    if pending.is_empty() {
        return;
    }
    let text = pending.join("\n\n");
    pending.clear();
    let timestamp = ts.unwrap_or_else(Utc::now);
    messages.push(UnifiedMessage {
        id: format!("thinking-{}", messages.len()),
        role: MessageRole::Assistant,
        content: vec![ContentBlock::Thinking { text }],
        timestamp,
        usage: None,
        duration_ms: None,
        model: None,
        completed_at: Some(timestamp),
    });
}

fn agents_instructions_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?s)\A# AGENTS\.md instructions for [^\n]+\n\s*\n<INSTRUCTIONS>.*?</INSTRUCTIONS>\s*",
        )
        .expect("valid agents instructions regex")
    })
}

fn strip_agents_instructions_block(input: &str) -> String {
    let text = agents_instructions_regex().replace(input, "");
    text.trim().to_string()
}

fn is_agents_instruction_message(input: &str) -> bool {
    input
        .trim_start()
        .starts_with("# AGENTS.md instructions for ")
}

fn is_environment_context_message(input: &str) -> bool {
    let trimmed = input.trim();
    trimmed.starts_with("<environment_context>") && trimmed.ends_with("</environment_context>")
}

/// codex re-injects `<codex_internal_context source="goal">Continue working …`
/// user turns while a `/goal` is active. These are machine context, never a real
/// prompt, so they must never become a conversation title (they otherwise leak in
/// on the summary path, whose title fallback doesn't gate on image blocks).
fn is_codex_internal_context_message(input: &str) -> bool {
    input.trim_start().starts_with("<codex_internal_context")
}

fn extract_codex_title_candidate(input: &str, fallback_attached: bool) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty()
        || is_agents_instruction_message(trimmed)
        || is_environment_context_message(trimmed)
        || is_codex_internal_context_message(trimmed)
    {
        return None;
    }

    let without_agents = strip_agents_instructions_block(trimmed);
    if without_agents.is_empty()
        || is_agents_instruction_message(&without_agents)
        || is_environment_context_message(&without_agents)
        || is_codex_internal_context_message(&without_agents)
    {
        return None;
    }

    let cleaned = strip_blocked_resource_mentions(&without_agents);
    if cleaned.is_empty() {
        if fallback_attached {
            Some("Attached resources".to_string())
        } else {
            None
        }
    } else {
        Some(title_from_user_text(&cleaned))
    }
}

fn extract_codex_text_content(payload: &serde_json::Value) -> Option<String> {
    let text = payload
        .get("content")?
        .as_array()?
        .iter()
        .filter(|item| item.get("type").and_then(|value| value.as_str()) == Some("input_text"))
        .filter_map(|item| item.get("text").and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

fn parse_data_uri_image(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("data:") {
        return None;
    }
    let marker = ";base64,";
    let marker_idx = trimmed.find(marker)?;
    let mime_type = trimmed.get(5..marker_idx)?.trim();
    if !mime_type.starts_with("image/") {
        return None;
    }
    let data = trimmed.get(marker_idx + marker.len()..)?.trim();
    if data.is_empty() {
        return None;
    }
    Some((mime_type.to_string(), data.to_string()))
}

fn input_image_url(item: &serde_json::Value) -> Option<&str> {
    item.get("image_url")
        .and_then(|v| v.as_str())
        .or_else(|| {
            item.get("image_url")
                .and_then(|v| v.get("url"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| item.get("url").and_then(|v| v.as_str()))
}

fn image_mime_from_url(url: &reqwest::Url) -> Option<&'static str> {
    let extension = url.path().rsplit('.').next()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

fn parse_image_reference(raw: &str) -> Option<ContentBlock> {
    let raw = raw.trim();
    if let Some((mime_type, data)) = parse_data_uri_image(raw) {
        return Some(ContentBlock::Image {
            data,
            mime_type,
            uri: None,
        });
    }
    let parsed = reqwest::Url::parse(raw).ok()?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    Some(ContentBlock::Image {
        data: String::new(),
        mime_type: image_mime_from_url(&parsed)?.to_string(),
        uri: Some(raw.to_string()),
    })
}

fn parse_input_image(item: &serde_json::Value) -> Option<ContentBlock> {
    parse_image_reference(input_image_url(item)?)
}

fn first_text_block(blocks: &[ContentBlock]) -> Option<String> {
    blocks.iter().find_map(|block| match block {
        ContentBlock::Text { text } => Some(text.clone()),
        _ => None,
    })
}

fn blocks_equal(a: &[ContentBlock], b: &[ContentBlock]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

fn should_skip_duplicate_user_message(
    messages: &[UnifiedMessage],
    blocks: &[ContentBlock],
    timestamp: DateTime<Utc>,
) -> bool {
    // Some Codex logs emit the same user message through both `response_item`
    // and `event_msg`, sometimes with a non-trivial delay. Deduplicate by
    // content in a bounded recent time window.
    const DUP_WINDOW_MS: i64 = 120_000;

    for msg in messages.iter().rev() {
        if !matches!(msg.role, MessageRole::User) {
            continue;
        }
        let delta_ms = (timestamp - msg.timestamp).num_milliseconds().abs();
        if delta_ms > DUP_WINDOW_MS {
            break;
        }
        if blocks_equal(&msg.content, blocks) {
            return true;
        }
    }

    false
}

/// Whether a `response_item` user message carries an `input_image` — the exact
/// condition under which [`extract_response_item_user_image_blocks`] yields a
/// real user turn in the detail parser. The lightweight summary parser uses this
/// to detect the same real-user-turn so its pure-`/goal` fallback stays in sync.
fn response_item_user_has_image(payload: &serde_json::Value) -> bool {
    payload
        .get("content")
        .and_then(|c| c.as_array())
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item.get("type").and_then(|v| v.as_str()) == Some("input_image"))
        })
}

fn extract_response_item_user_image_blocks(
    payload: &serde_json::Value,
) -> Option<Vec<ContentBlock>> {
    let content = payload.get("content")?.as_array()?;
    let mut blocks: Vec<ContentBlock> = Vec::new();
    let mut text_parts: Vec<String> = Vec::new();
    let mut has_input_image = false;

    for item in content {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match item_type {
            "input_text" => {
                let Some(text) = item.get("text").and_then(|v| v.as_str()) else {
                    continue;
                };
                if text.trim() == "<image>" {
                    continue;
                }
                if !text.is_empty() {
                    text_parts.push(text.to_string());
                }
            }
            "input_image" => {
                has_input_image = true;
                let Some(image) = parse_input_image(item) else {
                    continue;
                };
                blocks.push(image);
            }
            _ => {}
        }
    }

    if !has_input_image {
        return None;
    }

    let text = strip_blocked_resource_mentions(&text_parts.join("\n"));
    if !text.is_empty() {
        blocks.insert(0, ContentBlock::Text { text });
    }

    if blocks.is_empty() {
        blocks.push(ContentBlock::Text {
            text: "Attached resources".to_string(),
        });
    }

    Some(blocks)
}

fn strip_blocked_resource_mentions(input: &str) -> String {
    let blocked_re = Regex::new(r"@([^\s@]+)\s*\[blocked[^\]]*\]").expect("valid blocked regex");
    let image_tag_re = Regex::new(r"(?i)</?image\s*/?>").expect("valid image tag regex");
    let collapsed_ws_re = Regex::new(r"[ \t]{2,}").expect("valid whitespace regex");
    let text = blocked_re.replace_all(input, "").to_string();
    let text = image_tag_re.replace_all(&text, "").to_string();
    let text = collapsed_ws_re.replace_all(&text, " ").to_string();
    text.trim().to_string()
}

/// Group flat messages into conversation turns.
/// Codex rule: consecutive Assistant + Tool messages merge into one Assistant turn.
fn group_into_turns(messages: Vec<UnifiedMessage>) -> Vec<MessageTurn> {
    let mut turns = Vec::new();
    let mut i = 0;

    while i < messages.len() {
        let msg = &messages[i];

        if matches!(msg.role, MessageRole::User) {
            turns.push(MessageTurn {
                id: format!("turn-{}", turns.len()),
                role: TurnRole::User,
                blocks: msg.content.clone(),
                timestamp: msg.timestamp,
                usage: None,
                duration_ms: None,
                model: None,
                completed_at: msg.completed_at,
            });
            i += 1;
        } else if matches!(msg.role, MessageRole::System) {
            turns.push(MessageTurn {
                id: format!("turn-{}", turns.len()),
                role: TurnRole::System,
                blocks: msg.content.clone(),
                timestamp: msg.timestamp,
                usage: None,
                duration_ms: None,
                model: None,
                completed_at: msg.completed_at,
            });
            i += 1;
        } else {
            // Assistant or Tool — start a group
            let mut blocks: Vec<ContentBlock> = msg.content.clone();
            let mut usage = msg.usage.clone();
            let mut duration_ms = msg.duration_ms;
            let mut turn_model = msg.model.clone();
            let timestamp = msg.timestamp;
            let mut completed_at = msg.completed_at;
            i += 1;

            // Only absorb immediately following Tool messages
            // (stop at the next assistant message to keep turns small for virtualization)
            while i < messages.len() && matches!(messages[i].role, MessageRole::Tool) {
                blocks.extend(messages[i].content.clone());
                if usage.is_none() {
                    usage = messages[i].usage.clone();
                }
                if duration_ms.is_none() {
                    duration_ms = messages[i].duration_ms;
                }
                if turn_model.is_none() {
                    turn_model = messages[i].model.clone();
                }
                if messages[i].completed_at.is_some() {
                    completed_at = messages[i].completed_at;
                }
                i += 1;
            }

            turns.push(MessageTurn {
                id: format!("turn-{}", turns.len()),
                role: TurnRole::Assistant,
                blocks,
                timestamp,
                usage,
                duration_ms,
                model: turn_model,
                completed_at,
            });
        }
    }

    turns
}
