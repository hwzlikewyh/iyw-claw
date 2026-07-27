use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::models::{
    AgentExecutionStats, AgentToolCall, AgentType, ContentBlock, ConversationDetail,
    ConversationSummary, MessageRole, MessageTurn, TurnRole, TurnUsage, UnifiedMessage,
};
use crate::parsers::{
    compute_session_stats, folder_name_from_path, infer_context_window_max_tokens,
    is_safe_subagent_id, latest_turn_total_usage_tokens, merge_context_window_stats,
    relocate_orphaned_tool_results, resolve_patch_line_numbers, structurize_read_tool_output,
    title_from_user_text, truncate_str, AgentParser, ParseError,
};

/// Resolve Kimi Code's data home, honoring `KIMI_CODE_HOME`, else `~/.kimi-code`
/// (mirrors `resolve_codebuddy_config_dir`). The transcript store lives under
/// the `sessions/` subdirectory of this path.
pub(crate) fn resolve_kimi_code_home_dir() -> PathBuf {
    resolve_kimi_code_home_from(std::env::var_os("KIMI_CODE_HOME"), dirs::home_dir())
}

fn resolve_kimi_code_home_from(
    kimi_code_home_env: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> PathBuf {
    kimi_code_home_env
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir.unwrap_or_default().join(".kimi-code"))
}

/// Kimi Code (Moonshot AI) stores its session transcripts under a
/// **directory-per-session** layout — a third archetype distinct from CodeBuddy
/// (one JSONL file per session) and Hermes (a single SQLite DB):
///
/// ```text
/// $KIMI_CODE_HOME/                 (default ~/.kimi-code)
/// ├── config.toml
/// ├── session_index.jsonl          # {sessionId, sessionDir, workDir} per line
/// └── sessions/
///     └── <workDirKey>/            # bucketed by working directory (wd_<name>_<hash>)
///         └── <sessionId>/
///             ├── state.json        # {title, createdAt, updatedAt, agents, ...}
///             ├── logs/kimi-code.log
///             └── agents/
///                 ├── main/wire.jsonl       # the primary agent event stream
///                 └── agent-<n>/wire.jsonl  # sub-agent streams
/// ```
///
/// `base_dir` points at the `sessions/` directory.
///
/// `wire.jsonl` is an **event-sourcing log** (newline-delimited JSON), NOT an ACP
/// `session/update` stream. Each line has a top-level `type` and a millisecond
/// `time`. The records that carry conversation content are:
///
/// - `turn.prompt` — a user prompt (`input[]` of `{type:"text", text}` parts).
/// - `context.append_loop_event` — the assistant's work, where `event.type` is:
///   - `content.part` with `part.type` `"text"` (assistant message) or `"think"`
///     (reasoning, text under `part.think`),
///   - `tool.call` (`toolCallId` / `name` / `args`),
///   - `tool.result` (`toolCallId` / `result.output` / optional `result.isError`),
///   - `step.begin` / `step.end` (ignored; `step.end.usage` duplicates the
///     adjacent `usage.record`).
/// - `usage.record` — **per-step** token usage (`inputOther` / `output` /
///   `inputCacheRead` / `inputCacheCreation`); a turn's total is the sum of its
///   steps' records.
///
/// `context.append_message` records merely echo the prompt into the model context
/// (and carry `origin.kind == "injection"` system reminders), so they are skipped
/// to avoid duplicate / noise messages. The working directory is recovered from
/// `session_index.jsonl` (state.json has none); the model name from the session's
/// own `logs/kimi-code.log` (the wire only stores the iyw-claw-managed model alias).
pub struct KimiCodeParser {
    base_dir: PathBuf,
}

impl KimiCodeParser {
    pub fn new() -> Self {
        Self {
            base_dir: resolve_kimi_code_home_dir().join("sessions"),
        }
    }

    /// Construct a parser pointed at an explicit `sessions` directory (test
    /// fixtures).

    /// Load `session_index.jsonl` (sibling of `sessions/`) into a
    /// `sessionId → workDir` map. The index is the only source of a session's
    /// working directory (state.json does not record one). A missing or
    /// malformed index degrades to an empty map (cwd unknown).
    fn load_work_dir_index(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let Some(home) = self.base_dir.parent() else {
            return map;
        };
        let Ok(file) = fs::File::open(home.join("session_index.jsonl")) else {
            return map;
        };
        for line in BufReader::new(file).lines() {
            let Ok(line) = line else { continue };
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let session_id = value.get("sessionId").and_then(Value::as_str);
            let work_dir = value
                .get("workDir")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if let (Some(id), Some(dir)) = (session_id, work_dir) {
                map.insert(id.to_string(), dir.to_string());
            }
        }
        map
    }

    fn build_summary(
        &self,
        session_dir: &Path,
        session_id: &str,
        cwd: Option<String>,
    ) -> Option<ConversationSummary> {
        // The list view never renders sub-agent stats, so pass `None` to skip the
        // per-session sub-agent transcript I/O — only `build_detail` loads them.
        let parsed = parse_wire(&main_wire_path(session_dir), None);
        // A session that never produced a user/assistant/tool event (only the
        // metadata + system-prompt config records) is treated as empty, matching
        // the "metadata-only is not listed" rule of the other parsers.
        if parsed.content_events == 0 {
            return None;
        }
        let started_at = parsed.first_ts?;

        let model = read_session_log_model(session_dir).or_else(|| parsed.model_alias.clone());
        let folder_name = cwd
            .as_deref()
            .map(folder_name_from_path)
            .or_else(|| decode_work_dir_name(session_dir));

        Some(ConversationSummary {
            id: session_id.to_string(),
            agent_type: AgentType::KimiCode,
            folder_path: cwd,
            folder_name,
            title: resolve_title(read_state_title(session_dir), parsed.first_user_text),
            started_at,
            ended_at: parsed.last_ts,
            message_count: parsed.message_count,
            model,
            git_branch: None,
            parent_id: None,
            parent_tool_use_id: None,
            delegation_call_id: None,
        })
    }

    fn build_detail(
        &self,
        session_dir: &Path,
        conversation_id: &str,
        cwd: Option<String>,
    ) -> ConversationDetail {
        // `agents/` holds both the main wire and each sub-agent's wire, so an
        // `Agent` delegation result can load its sub-agent transcript from here.
        let parsed = parse_wire(
            &main_wire_path(session_dir),
            Some(&session_dir.join("agents")),
        );

        let mut turns = group_into_turns(parsed.messages);
        relocate_orphaned_tool_results(&mut turns);
        structurize_read_tool_output(&mut turns);
        resolve_patch_line_numbers(&mut turns, cwd.as_deref());

        let model = read_session_log_model(session_dir).or_else(|| parsed.model_alias.clone());
        let used_tokens = latest_turn_total_usage_tokens(&turns);
        let max_tokens = infer_context_window_max_tokens(model.as_deref());
        let session_stats =
            merge_context_window_stats(compute_session_stats(&turns), used_tokens, max_tokens);

        let folder_name = cwd
            .as_deref()
            .map(folder_name_from_path)
            .or_else(|| decode_work_dir_name(session_dir));

        let summary = ConversationSummary {
            id: conversation_id.to_string(),
            agent_type: AgentType::KimiCode,
            folder_path: cwd,
            folder_name,
            title: resolve_title(read_state_title(session_dir), parsed.first_user_text),
            started_at: parsed.first_ts.unwrap_or_else(Utc::now),
            ended_at: parsed.last_ts,
            message_count: parsed.message_count,
            model,
            git_branch: None,
            parent_id: None,
            parent_tool_use_id: None,
            delegation_call_id: None,
        };

        ConversationDetail {
            summary,
            turns,
            session_stats,
            transcript_watermark: None,
        }
    }

    /// Locate the `<sessionId>` directory matching `conversation_id` across the
    /// `base_dir/<workDirKey>/` buckets (two shallow levels; never descends into
    /// `agents/`).
    fn find_session_dir(&self, conversation_id: &str) -> Option<PathBuf> {
        for bucket in read_subdirs(&self.base_dir) {
            let candidate = bucket.join(conversation_id);
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
        None
    }
}

impl Default for KimiCodeParser {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentParser for KimiCodeParser {
    fn list_conversations(&self) -> Result<Vec<ConversationSummary>, ParseError> {
        let mut conversations = Vec::new();
        if !self.base_dir.is_dir() {
            return Ok(conversations);
        }
        let index = self.load_work_dir_index();

        for bucket in read_subdirs(&self.base_dir) {
            for session_dir in read_subdirs(&bucket) {
                let Some(session_id) = session_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                else {
                    continue;
                };
                let cwd = index.get(&session_id).cloned();
                if let Some(summary) = self.build_summary(&session_dir, &session_id, cwd) {
                    conversations.push(summary);
                }
            }
        }

        conversations.sort_by_key(|c| std::cmp::Reverse(c.started_at));
        Ok(conversations)
    }

    fn get_conversation(&self, conversation_id: &str) -> Result<ConversationDetail, ParseError> {
        let Some(session_dir) = self.find_session_dir(conversation_id) else {
            return Err(ParseError::ConversationNotFound(
                conversation_id.to_string(),
            ));
        };
        let cwd = self.load_work_dir_index().get(conversation_id).cloned();
        Ok(self.build_detail(&session_dir, conversation_id, cwd))
    }
}

/// The accumulated result of scanning one agent's `wire.jsonl`.
#[derive(Default)]
struct WireParse {
    messages: Vec<UnifiedMessage>,
    first_ts: Option<DateTime<Utc>>,
    last_ts: Option<DateTime<Utc>>,
    /// The iyw-claw-managed model alias from a `config.update` record (fallback only;
    /// the real model name is recovered from the session log).
    model_alias: Option<String>,
    /// First user prompt, already truncated for use as a fallback title.
    first_user_text: Option<String>,
    /// User + assistant-text messages (tool calls/results and thinking excluded),
    /// a coarse activity count for the list view.
    message_count: u32,
    /// Number of content-bearing records — used to decide whether the session is
    /// worth listing at all.
    content_events: u32,
}

fn main_wire_path(session_dir: &Path) -> PathBuf {
    session_dir.join("agents").join("main").join("wire.jsonl")
}

/// Parse a `wire.jsonl` event stream into a flat, chronologically-ordered list of
/// `UnifiedMessage`s plus session metadata. Unknown / malformed lines are skipped
/// (`continue`) so a forward-compatible or partially-written log never panics.
///
/// When `agents_dir` is `Some` (the conversation-detail path), an `Agent`
/// delegation's tool result loads the sub-agent's own `wire.jsonl` from
/// `<agents_dir>/<agent_id>/` and attaches its nested tool calls as `agent_stats`
/// so the sub-agent renders as an expandable Agent pill. `None` (the list path)
/// skips that per-session I/O entirely.
fn parse_wire(path: &Path, agents_dir: Option<&Path>) -> WireParse {
    let mut wp = WireParse::default();
    let Ok(file) = fs::File::open(path) else {
        return wp;
    };

    // Per-step `usage.record`s accumulate into the turn's total, then flush onto
    // the turn's last assistant message at the next `turn.prompt` (or EOF).
    let mut pending_usage: Option<TurnUsage> = None;
    let mut last_assistant_idx: Option<usize> = None;
    // `toolCallId`s of `tool.call`s classified as `Agent` delegations. Only their
    // paired results may load a sub-agent transcript, so an ordinary tool result
    // can never gain `agent_stats` (mirrors CodeBuddy's `agent_call_ids` gate).
    let mut agent_call_ids: HashSet<String> = HashSet::new();

    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let record_type = value.get("type").and_then(Value::as_str).unwrap_or("");
        let ts_raw = event_millis(&value);

        match record_type {
            "config.update" => {
                if let Some(alias) = value
                    .get("modelAlias")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    wp.model_alias.get_or_insert_with(|| alias.to_string());
                }
            }
            "turn.prompt" => {
                flush_usage(
                    &mut wp.messages,
                    &mut pending_usage,
                    &mut last_assistant_idx,
                );
                let text = collect_prompt_text(&value);
                if text.trim().is_empty() {
                    continue;
                }
                let ts = note_content_ts(&mut wp, ts_raw);
                if wp.first_user_text.is_none() {
                    wp.first_user_text = Some(title_from_user_text(text.trim()));
                }
                wp.content_events += 1;
                wp.message_count += 1;
                wp.messages.push(text_message(
                    format!("kc-user-{idx}"),
                    MessageRole::User,
                    text,
                    ts,
                ));
            }
            "context.append_loop_event" => {
                let Some(event) = value.get("event") else {
                    continue;
                };
                let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");
                match event_type {
                    "content.part" => {
                        let part = event.get("part").cloned().unwrap_or(Value::Null);
                        match part.get("type").and_then(Value::as_str).unwrap_or("") {
                            "text" => {
                                let text = part_text(&part, "text");
                                if text.trim().is_empty() {
                                    continue;
                                }
                                let ts = note_content_ts(&mut wp, ts_raw);
                                wp.content_events += 1;
                                wp.message_count += 1;
                                wp.messages.push(text_message(
                                    format!("kc-text-{idx}"),
                                    MessageRole::Assistant,
                                    text,
                                    ts,
                                ));
                                last_assistant_idx = Some(wp.messages.len() - 1);
                            }
                            "think" => {
                                let text = part_text(&part, "think");
                                if text.trim().is_empty() {
                                    continue;
                                }
                                let ts = note_content_ts(&mut wp, ts_raw);
                                wp.content_events += 1;
                                wp.messages.push(block_message(
                                    format!("kc-think-{idx}"),
                                    MessageRole::Assistant,
                                    ContentBlock::Thinking { text },
                                    ts,
                                ));
                                last_assistant_idx = Some(wp.messages.len() - 1);
                            }
                            _ => {}
                        }
                    }
                    "tool.call" => {
                        let ts = note_content_ts(&mut wp, ts_raw);
                        wp.content_events += 1;
                        let tool_call_id = event
                            .get("toolCallId")
                            .and_then(Value::as_str)
                            .map(String::from);
                        // Record `Agent` delegations so only their paired results
                        // load a sub-agent transcript (the gate is applied below).
                        if is_agent_tool_call(event) {
                            if let Some(id) = &tool_call_id {
                                agent_call_ids.insert(id.clone());
                            }
                        }
                        wp.messages.push(block_message(
                            format!("kc-toolcall-{idx}"),
                            MessageRole::Assistant,
                            ContentBlock::ToolUse {
                                tool_use_id: tool_call_id,
                                tool_name: event
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("unknown")
                                    .to_string(),
                                input_preview: tool_args_preview(event),
                                meta: None,
                            },
                            ts,
                        ));
                        last_assistant_idx = Some(wp.messages.len() - 1);
                    }
                    "tool.result" => {
                        let ts = note_content_ts(&mut wp, ts_raw);
                        wp.content_events += 1;
                        let result = event.get("result");
                        let tool_call_id = event
                            .get("toolCallId")
                            .and_then(Value::as_str)
                            .map(String::from);
                        let output_preview = result.and_then(tool_result_preview);
                        // Load the sub-agent transcript only for a result paired
                        // (by `toolCallId`) to a `tool.call` classified as an
                        // `Agent` delegation. Every ordinary result stays `None`,
                        // even one whose output coincidentally opens with an
                        // `agent_id:` line — the gate is the call classification,
                        // not the marker's presence.
                        let agent_stats = agents_dir
                            .filter(|_| {
                                tool_call_id
                                    .as_deref()
                                    .is_some_and(|id| agent_call_ids.contains(id))
                            })
                            .and_then(|dir| {
                                agent_stats_from_subagent(output_preview.as_deref(), dir)
                            });
                        wp.messages.push(block_message(
                            format!("kc-toolresult-{idx}"),
                            MessageRole::Tool,
                            ContentBlock::ToolResult {
                                tool_use_id: tool_call_id,
                                output_preview,
                                is_error: result
                                    .and_then(|r| r.get("isError"))
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                                agent_stats,
                                // Kimi tool results are text/JSON today; image
                                // capture (cf. main's tool-result image support)
                                // is a follow-up that needs a real image sample.
                                images: Vec::new(),
                            },
                            ts,
                        ));
                    }
                    _ => {} // step.begin / step.end carry no renderable content
                }
            }
            "usage.record" => {
                if let Some(usage) = usage_from_record(value.get("usage")) {
                    pending_usage = Some(match pending_usage.take() {
                        Some(prev) => add_usage(prev, usage),
                        None => usage,
                    });
                }
            }
            _ => {}
        }
    }

    flush_usage(
        &mut wp.messages,
        &mut pending_usage,
        &mut last_assistant_idx,
    );
    wp
}

/// Record a content event's timestamp into the session span and return a concrete
/// timestamp for the message (falling back to the last seen one, then now).
fn note_content_ts(wp: &mut WireParse, ts_raw: Option<DateTime<Utc>>) -> DateTime<Utc> {
    if let Some(ts) = ts_raw {
        wp.first_ts.get_or_insert(ts);
        wp.last_ts = Some(ts);
    }
    ts_raw.or(wp.last_ts).unwrap_or_else(Utc::now)
}

/// Attach the accumulated per-turn usage to the turn's last assistant message and
/// reset the accumulator for the next turn.
fn flush_usage(
    messages: &mut [UnifiedMessage],
    pending: &mut Option<TurnUsage>,
    last_assistant_idx: &mut Option<usize>,
) {
    if let (Some(usage), Some(i)) = (pending.take(), *last_assistant_idx) {
        if let Some(message) = messages.get_mut(i) {
            message.usage = Some(match message.usage.take() {
                Some(existing) => add_usage(existing, usage),
                None => usage,
            });
        }
    }
    *last_assistant_idx = None;
}

/// Top-level millisecond `time` → `DateTime<Utc>` (Kimi uses numeric epoch ms).
fn event_millis(value: &Value) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(value.get("time")?.as_i64()?)
}

/// Concatenate the `text` of every `{type:"text"}` part in a `turn.prompt.input`.
fn collect_prompt_text(value: &Value) -> String {
    let mut out = String::new();
    if let Some(items) = value.get("input").and_then(Value::as_array) {
        for item in items {
            if item.get("type").and_then(Value::as_str) == Some("text") {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    out.push_str(text);
                }
            }
        }
    }
    out
}

/// Pull a string field (`text` or `think`) out of a `content.part`.
fn part_text(part: &Value, key: &str) -> String {
    part.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `tool.call.args` is an object (e.g. `{command, cwd, timeout}`); serialize it
/// for the input preview, defensively accepting a pre-stringified value.
fn tool_args_preview(event: &Value) -> Option<String> {
    let args = event.get("args")?;
    if let Some(text) = args.as_str() {
        (!text.is_empty()).then(|| text.to_string())
    } else if args.is_null() {
        None
    } else {
        serde_json::to_string(args).ok()
    }
}

/// `tool.result.result.output` is usually a string; rich outputs (e.g. images)
/// arrive as an array/object, which is serialized as a fallback.
fn tool_result_preview(result: &Value) -> Option<String> {
    let output = result.get("output")?;
    if let Some(text) = output.as_str() {
        (!text.is_empty()).then(|| text.to_string())
    } else if output.is_null() {
        None
    } else {
        serde_json::to_string(output).ok()
    }
}

/// True when a `tool.call` event is an `Agent` sub-agent delegation — its `name`
/// is `"Agent"`, or (defensively) its `args` carry a non-empty `subagent_type`.
/// Only such calls' paired results may load a sub-agent transcript.
fn is_agent_tool_call(event: &Value) -> bool {
    if event.get("name").and_then(Value::as_str) == Some("Agent") {
        return true;
    }
    event
        .get("args")
        .and_then(|args| args.get("subagent_type"))
        .and_then(Value::as_str)
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// The sub-agent transcript id Kimi writes at the head of an `Agent` tool
/// result's output: the first line is `agent_id: agent-0` (followed by
/// `actual_subagent_type:` / `status:` / a blank line / the summary). Only the
/// first line is inspected, so an `agent_id:` substring appearing inside the
/// summary body can never be mistaken for the marker. Returns `None` for an
/// ordinary result whose output carries no such header.
fn subagent_id_from_output(output: &str) -> Option<&str> {
    output
        .lines()
        .next()?
        .trim()
        .strip_prefix("agent_id:")
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

/// Walk a sub-agent's `wire.jsonl` — the same event-sourcing format as the main
/// wire — and extract its tool calls as `AgentToolCall`s, pairing each
/// `tool.call` with its `tool.result` by `toolCallId`. The outer
/// `tool_args_preview` / `tool_result_preview` helpers are reused so nested calls
/// render identically to top-level ones. Mirrors `codebuddy.rs`'s
/// `parse_codebuddy_subagent_tool_calls`.
///
/// Intentionally non-recursive: a nested `Agent` call inside the sub-agent shows
/// as a flat leaf tool here (no further descent), which bounds the work and
/// matches the frontend stripping `agent_stats` from nested renders.
fn parse_kimi_subagent_tool_calls(path: &Path) -> Vec<AgentToolCall> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };

    // (toolCallId, name, input) in encounter order, paired against results by id.
    let mut calls: Vec<(Option<String>, String, Option<String>)> = Vec::new();
    let mut results: HashMap<String, (Option<String>, bool)> = HashMap::new();

    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("context.append_loop_event") {
            continue;
        }
        let Some(event) = value.get("event") else {
            continue;
        };
        match event.get("type").and_then(Value::as_str).unwrap_or("") {
            "tool.call" => {
                calls.push((
                    event
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .map(String::from),
                    event
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string(),
                    tool_args_preview(event).map(|s| truncate_str(&s, 500)),
                ));
            }
            "tool.result" => {
                if let Some(id) = event.get("toolCallId").and_then(Value::as_str) {
                    let result = event.get("result");
                    let output = result
                        .and_then(tool_result_preview)
                        .map(|s| truncate_str(&s, 500));
                    let is_error = result
                        .and_then(|r| r.get("isError"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    results.insert(id.to_string(), (output, is_error));
                }
            }
            _ => {}
        }
    }

    calls
        .into_iter()
        .map(|(id, tool_name, input_preview)| {
            let (output_preview, is_error) =
                id.and_then(|i| results.remove(&i)).unwrap_or((None, false));
            AgentToolCall {
                tool_name,
                input_preview,
                output_preview,
                is_error,
            }
        })
        .collect()
}

/// Build `agent_stats` for an `Agent` tool result by reading the sub-agent id
/// from its output header (`subagent_id_from_output`) and loading the sub-agent's
/// own `wire.jsonl` from `<agents_dir>/<agent_id>/`. The historical mirror of the
/// live path, which synthesizes the same `agent_stats` from the streamed child
/// tool calls.
///
/// Returns `None` for an ordinary result (no `agent_id:` header), an unsafe id, a
/// missing transcript, or a sub-agent that ran no tools — so the common case
/// stays a plain tool result.
fn agent_stats_from_subagent(
    output: Option<&str>,
    agents_dir: &Path,
) -> Option<AgentExecutionStats> {
    let id = subagent_id_from_output(output?)?;
    // `id` becomes a path component under `agents_dir`; reject anything that
    // could escape the directory before a file is opened.
    if !is_safe_subagent_id(id) {
        return None;
    }
    let transcript = agents_dir.join(id).join("wire.jsonl");
    if !transcript.exists() {
        return None;
    }
    let tool_calls = parse_kimi_subagent_tool_calls(&transcript);
    if tool_calls.is_empty() {
        return None;
    }
    let tool_count = tool_calls.len() as u32;
    Some(AgentExecutionStats {
        agent_type: None,
        status: None,
        total_duration_ms: None,
        total_tokens: None,
        total_tool_use_count: Some(tool_count),
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

/// Map a `usage.record.usage` object onto `TurnUsage`; `None` when all counters
/// are absent or zero so empty records do not create spurious usage.
fn usage_from_record(usage: Option<&Value>) -> Option<TurnUsage> {
    let usage = usage?;
    let field = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    let input = field("inputOther");
    let output = field("output");
    let cache_read = field("inputCacheRead");
    let cache_creation = field("inputCacheCreation");
    if input == 0 && output == 0 && cache_read == 0 && cache_creation == 0 {
        return None;
    }
    Some(TurnUsage {
        input_tokens: input,
        output_tokens: output,
        cache_creation_input_tokens: cache_creation,
        cache_read_input_tokens: cache_read,
    })
}

fn add_usage(a: TurnUsage, b: TurnUsage) -> TurnUsage {
    TurnUsage {
        input_tokens: a.input_tokens.saturating_add(b.input_tokens),
        output_tokens: a.output_tokens.saturating_add(b.output_tokens),
        cache_creation_input_tokens: a
            .cache_creation_input_tokens
            .saturating_add(b.cache_creation_input_tokens),
        cache_read_input_tokens: a
            .cache_read_input_tokens
            .saturating_add(b.cache_read_input_tokens),
    }
}

fn text_message(id: String, role: MessageRole, text: String, ts: DateTime<Utc>) -> UnifiedMessage {
    block_message(id, role, ContentBlock::Text { text }, ts)
}

fn block_message(
    id: String,
    role: MessageRole,
    block: ContentBlock,
    ts: DateTime<Utc>,
) -> UnifiedMessage {
    UnifiedMessage {
        id,
        role,
        content: vec![block],
        timestamp: ts,
        usage: None,
        duration_ms: None,
        model: None,
        completed_at: Some(ts),
    }
}

/// Read `state.json`'s `title`, ignoring the placeholder "New Session" so the
/// caller can fall back to the first user prompt.
fn read_state_title(session_dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(session_dir.join("state.json")).ok()?;
    let value = serde_json::from_str::<Value>(&raw).ok()?;
    value
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "New Session")
        .map(String::from)
}

fn resolve_title(state_title: Option<String>, first_user_text: Option<String>) -> Option<String> {
    state_title.or(first_user_text)
}

/// Best-effort real model name from the session's own log
/// (`… llm config … model=kimi-k2.7-code modelAlias=iyw-claw-managed …`). The wire
/// only stores the (iyw-claw-managed) alias, so the log is the sole place the actual
/// model id appears. `modelAlias=` does not collide: only the exact `model=`
/// token is matched.
fn read_session_log_model(session_dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(session_dir.join("logs").join("kimi-code.log")).ok()?;
    for line in raw.lines() {
        if !line.contains("llm config") {
            continue;
        }
        for token in line.split_whitespace() {
            if let Some(model) = token.strip_prefix("model=") {
                let model = model.trim();
                if !model.is_empty() {
                    return Some(model.to_string());
                }
            }
        }
    }
    None
}

/// Recover a folder *label* from the `wd_<name>_<hash>` bucket directory when the
/// real working directory is unknown (no `session_index.jsonl` entry). The hash
/// is one-way, so only the human-readable name is recovered — never a fabricated
/// path. Returns `None` if the bucket does not follow the `wd_…` convention.
fn decode_work_dir_name(session_dir: &Path) -> Option<String> {
    let bucket = session_dir.parent()?.file_name()?.to_str()?;
    let rest = bucket.strip_prefix("wd_")?;
    // Drop the trailing `_<hash>` segment; tolerate names containing underscores.
    rest.rsplit_once('_')
        .map(|(name, _hash)| name)
        .filter(|name| !name.is_empty())
        .map(String::from)
}

/// List immediate sub-directories of `dir` (empty when `dir` is missing or not a
/// directory). Shallow by design — the layout is exactly two levels deep and we
/// must not descend into `agents/`.
fn read_subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

/// Group the flat, chronologically-ordered `UnifiedMessage`s into `MessageTurn`s:
/// User/System messages each become their own turn; an Assistant message starts a
/// turn that absorbs the immediately-following Tool messages (its tool results),
/// stopping at the next Assistant message to keep turns small for virtualization.
/// (Private copy mirroring the other directory-layout parsers.)
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
            // Assistant or Tool — start a group and absorb following Tool messages.
            let mut blocks: Vec<ContentBlock> = msg.content.clone();
            let mut usage = msg.usage.clone();
            let mut duration_ms = msg.duration_ms;
            let mut turn_model = msg.model.clone();
            let timestamp = msg.timestamp;
            let mut completed_at = msg.completed_at;
            i += 1;

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

