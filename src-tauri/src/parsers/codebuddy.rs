use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;
use walkdir::WalkDir;

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

/// Resolve CodeBuddy's config dir, honoring `CODEBUDDY_CONFIG_DIR`, else
/// `~/.codebuddy` (mirrors `resolve_claude_config_dir`).
pub(crate) fn resolve_codebuddy_config_dir() -> PathBuf {
    resolve_codebuddy_config_dir_from(std::env::var_os("CODEBUDDY_CONFIG_DIR"), dirs::home_dir())
}

fn resolve_codebuddy_config_dir_from(
    codebuddy_config_dir_env: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> PathBuf {
    codebuddy_config_dir_env
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir.unwrap_or_default().join(".codebuddy"))
}

/// CodeBuddy (Tencent Cloud) stores its transcripts under
/// `~/.codebuddy/projects/<encoded-cwd>/<sessionId>.jsonl`, borrowing Claude
/// Code's *directory layout* — but the per-line record schema is the OpenAI
/// Agents SDK "items" shape, NOT Claude's: top-level `type`
/// (`message`/`reasoning`/`function_call`/`function_call_result`/`ai-title`/…),
/// a top-level `role` with a `content[]` array of `input_text`/`output_text`
/// items, and millisecond-epoch timestamps. So this parser reads those records
/// directly rather than reusing the Claude parser.
pub struct CodeBuddyParser {
    base_dir: PathBuf,
}

impl CodeBuddyParser {
    pub fn new() -> Self {
        Self {
            base_dir: resolve_codebuddy_config_dir().join("projects"),
        }
    }

    /// Construct a parser pointed at an explicit `projects` directory (test
    /// fixtures).

    fn parse_summary(&self, path: &Path) -> Option<ConversationSummary> {
        let reader = BufReader::new(fs::File::open(path).ok()?);

        let mut first_ts: Option<DateTime<Utc>> = None;
        let mut last_ts: Option<DateTime<Utc>> = None;
        let mut ai_title: Option<String> = None;
        let mut first_user_text: Option<String> = None;
        let mut model: Option<String> = None;
        let mut cwd: Option<String> = None;
        let mut session_id: Option<String> = None;
        let mut message_count: u32 = 0;

        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };

            let record_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if is_content_record(record_type) {
                if let Some(ts) = record_millis(&value) {
                    first_ts.get_or_insert(ts);
                    last_ts = Some(ts);
                }
            }
            if cwd.is_none() {
                cwd = record_cwd(&value);
            }
            if session_id.is_none() {
                session_id = value
                    .get("sessionId")
                    .and_then(|s| s.as_str())
                    .map(String::from);
            }
            if model.is_none() {
                model = record_model(&value);
            }

            match record_type {
                "ai-title" => {
                    if ai_title.is_none() {
                        ai_title = value
                            .get("aiTitle")
                            .and_then(|t| t.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(String::from);
                    }
                }
                "message" => match value.get("role").and_then(|r| r.as_str()).unwrap_or("") {
                    "user" => {
                        message_count += 1;
                        if first_user_text.is_none() {
                            let text = collect_text(&value, "input_text");
                            if !text.trim().is_empty() {
                                first_user_text = Some(title_from_user_text(text.trim()));
                            }
                        }
                    }
                    "assistant" => message_count += 1,
                    _ => {}
                },
                _ => {}
            }
        }

        let started_at = first_ts?;
        let id = session_id.unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        let folder_name = cwd.as_deref().map(folder_name_from_path);

        Some(ConversationSummary {
            id,
            agent_type: AgentType::CodeBuddy,
            folder_path: cwd,
            folder_name,
            title: ai_title.or(first_user_text),
            started_at,
            ended_at: last_ts,
            message_count,
            model,
            git_branch: None,
            parent_id: None,
            parent_tool_use_id: None,
            delegation_call_id: None,
        })
    }

    fn parse_detail(
        &self,
        path: &Path,
        conversation_id: &str,
    ) -> Result<ConversationDetail, ParseError> {
        let reader = BufReader::new(fs::File::open(path)?);

        let mut messages: Vec<UnifiedMessage> = Vec::new();
        let mut first_ts: Option<DateTime<Utc>> = None;
        let mut last_ts: Option<DateTime<Utc>> = None;
        let mut ai_title: Option<String> = None;
        let mut first_user_text: Option<String> = None;
        let mut model: Option<String> = None;
        let mut cwd: Option<String> = None;
        let mut message_count: u32 = 0;
        // `callId`s of `function_call`s classified as an `Agent` delegation. Only
        // their paired results may load a sub-agent transcript — so an ordinary
        // tool result that happens to carry a `subAgent` block (corruption,
        // schema drift, a future tool) can never gain `agent_stats`. Uses the
        // same agent classification as the tool-use rename, so it tracks "Agent"
        // and the Claude-style "Task"+subagent_type form alike.
        let mut agent_call_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for (idx, line) in reader.lines().enumerate() {
            let Ok(line) = line else { continue };
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };

            let record_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let ts_raw = record_millis(&value);
            if is_content_record(record_type) {
                if let Some(ts) = ts_raw {
                    first_ts.get_or_insert(ts);
                    last_ts = Some(ts);
                }
            }
            let ts = ts_raw.or(last_ts).unwrap_or_else(Utc::now);

            if cwd.is_none() {
                cwd = record_cwd(&value);
            }
            if model.is_none() {
                model = record_model(&value);
            }

            match record_type {
                "ai-title" => {
                    if ai_title.is_none() {
                        ai_title = value
                            .get("aiTitle")
                            .and_then(|t| t.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(String::from);
                    }
                }
                "message" => match value.get("role").and_then(|r| r.as_str()).unwrap_or("") {
                    "user" => {
                        message_count += 1;
                        let text = collect_text(&value, "input_text");
                        if first_user_text.is_none() && !text.trim().is_empty() {
                            first_user_text = Some(title_from_user_text(text.trim()));
                        }
                        if !text.trim().is_empty() {
                            messages.push(text_message(
                                format!("cb-user-{idx}"),
                                MessageRole::User,
                                text,
                                ts,
                                None,
                                None,
                            ));
                        }
                    }
                    "assistant" => {
                        message_count += 1;
                        let text = collect_text(&value, "output_text");
                        if !text.trim().is_empty() {
                            messages.push(text_message(
                                format!("cb-assistant-{idx}"),
                                MessageRole::Assistant,
                                text,
                                ts,
                                usage_from_raw(&value),
                                record_model(&value),
                            ));
                        }
                    }
                    _ => {}
                },
                "reasoning" => {
                    let text = reasoning_text(&value);
                    if !text.trim().is_empty() {
                        messages.push(UnifiedMessage {
                            id: format!("cb-reasoning-{idx}"),
                            role: MessageRole::Assistant,
                            content: vec![ContentBlock::Thinking { text }],
                            timestamp: ts,
                            usage: None,
                            duration_ms: None,
                            model: record_model(&value),
                            completed_at: Some(ts),
                        });
                    }
                }
                "function_call" => {
                    let tool_call_id = call_id(&value);
                    let tool_name = resolve_tool_call_name(&value);
                    if tool_name == "Agent" {
                        if let Some(id) = &tool_call_id {
                            agent_call_ids.insert(id.clone());
                        }
                    }
                    messages.push(UnifiedMessage {
                        id: format!("cb-toolcall-{idx}"),
                        role: MessageRole::Assistant,
                        content: vec![ContentBlock::ToolUse {
                            tool_use_id: tool_call_id,
                            tool_name,
                            input_preview: tool_input_preview(&value),
                            meta: None,
                        }],
                        timestamp: ts,
                        usage: None,
                        duration_ms: None,
                        model: None,
                        completed_at: Some(ts),
                    });
                }
                "function_call_result" => {
                    let tool_call_id = call_id(&value);
                    // Load the sub-agent transcript only for a result paired (by
                    // callId) to a `function_call` we classified as an `Agent`
                    // delegation — the historical mirror of the live path. Every
                    // ordinary tool result stays `None`, even one that carries a
                    // stray `subAgent` block, so non-Agent results are unchanged.
                    let agent_stats = tool_call_id
                        .as_deref()
                        .is_some_and(|id| agent_call_ids.contains(id))
                        .then(|| agent_stats_from_subagent(&value, path))
                        .flatten();
                    messages.push(UnifiedMessage {
                        id: format!("cb-toolresult-{idx}"),
                        role: MessageRole::Tool,
                        content: vec![ContentBlock::ToolResult {
                            tool_use_id: tool_call_id,
                            output_preview: tool_output_preview(&value),
                            is_error: tool_is_error(&value),
                            agent_stats,
                            images: Vec::new(),
                        }],
                        timestamp: ts,
                        usage: None,
                        duration_ms: None,
                        model: None,
                        completed_at: Some(ts),
                    });
                }
                _ => {}
            }
        }

        let mut turns = group_into_turns(messages);
        relocate_orphaned_tool_results(&mut turns);
        structurize_read_tool_output(&mut turns);
        resolve_patch_line_numbers(&mut turns, cwd.as_deref());

        let used_tokens = latest_turn_total_usage_tokens(&turns);
        let max_tokens = infer_context_window_max_tokens(model.as_deref());
        let session_stats =
            merge_context_window_stats(compute_session_stats(&turns), used_tokens, max_tokens);

        let folder_name = cwd.as_deref().map(folder_name_from_path);
        let summary = ConversationSummary {
            id: conversation_id.to_string(),
            agent_type: AgentType::CodeBuddy,
            folder_path: cwd,
            folder_name,
            title: ai_title.or(first_user_text),
            started_at: first_ts.unwrap_or_else(Utc::now),
            ended_at: last_ts,
            message_count,
            model,
            git_branch: None,
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

impl Default for CodeBuddyParser {
    fn default() -> Self {
        Self::new()
    }
}

/// True when `path` is a CodeBuddy sub-agent transcript rather than a top-level
/// session, so the conversation scan can skip it — otherwise a sub-agent's
/// internal execution transcript would surface as a bogus top-level conversation
/// (and `get_conversation` would open it). It only feeds an Agent result's
/// `agent_stats` (loaded by constructed path in `agent_stats_from_subagent`, not
/// via this scan), so hiding it from the list is safe.
///
/// CodeBuddy's documented layout is `<projects>/<encoded-cwd>/<sessionId>.jsonl`
/// for a top-level session and `<encoded-cwd>/<sessionId>/subagents/<agent>.jsonl`
/// for a sub-agent transcript. So a transcript is a `.jsonl` whose immediate
/// parent directory is `subagents`, nested at least that deep
/// (encoded-cwd + session + `subagents` + file ⇒ ≥ 4 components below
/// `base_dir`). The depth floor is what keeps a *legitimate* session whose own
/// encoded-cwd dir is literally named `subagents`
/// (`<projects>/subagents/<sessionId>.jsonl`, only 2 components) from being
/// mistaken for one. Computed on the components below `base_dir` so a `subagents`
/// segment in the base path's own prefix can't over-match either.
fn is_subagent_transcript(base_dir: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(base_dir).unwrap_or(path);
    let components: Vec<_> = relative.components().collect();
    components.len() >= 4 && components[components.len() - 2].as_os_str() == "subagents"
}

impl AgentParser for CodeBuddyParser {
    fn list_conversations(&self) -> Result<Vec<ConversationSummary>, ParseError> {
        let mut conversations = Vec::new();
        if !self.base_dir.exists() {
            return Ok(conversations);
        }

        for entry in WalkDir::new(&self.base_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            // A sub-agent transcript (`<session>/subagents/<agent>.jsonl`) is not
            // a top-level conversation; it only feeds an Agent result's
            // `agent_stats`. Skip it so the history list isn't polluted.
            if is_subagent_transcript(&self.base_dir, path) {
                continue;
            }
            if let Ok(Some(summary)) =
                super::summary_cache::get_or_parse(AgentType::CodeBuddy, path, || {
                    Ok(self.parse_summary(path))
                })
            {
                conversations.push(summary);
            }
        }

        conversations.sort_by_key(|c| std::cmp::Reverse(c.started_at));
        Ok(conversations)
    }

    fn get_conversation(&self, conversation_id: &str) -> Result<ConversationDetail, ParseError> {
        if self.base_dir.exists() {
            for entry in WalkDir::new(&self.base_dir)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                // Never open a sub-agent transcript as a top-level conversation,
                // even if its file stem happens to match the requested id.
                if is_subagent_transcript(&self.base_dir, path) {
                    continue;
                }
                if path.file_stem().map(|s| s.to_string_lossy()).as_deref() == Some(conversation_id)
                {
                    return self.parse_detail(path, conversation_id);
                }
            }
        }

        Err(ParseError::ConversationNotFound(
            conversation_id.to_string(),
        ))
    }
}

/// Epoch-millisecond `timestamp` → `DateTime<Utc>` (CodeBuddy uses numeric ms,
/// not Claude's ISO strings).
fn record_millis(value: &Value) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(value.get("timestamp")?.as_i64()?)
}

fn record_cwd(value: &Value) -> Option<String> {
    value
        .get("cwd")
        .and_then(|c| c.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Record types that carry actual conversation content, as opposed to the
/// `ai-title` / `summary` / `file-history-snapshot` metadata records (which also
/// carry timestamps). Only content records define the session's
/// `started_at`/`ended_at` span and whether a transcript is listed at all — so a
/// metadata-only file is treated as empty rather than surfacing as a
/// zero-message conversation.
fn is_content_record(record_type: &str) -> bool {
    matches!(
        record_type,
        "message" | "reasoning" | "function_call" | "function_call_result"
    )
}

/// Display model name from `providerData`: prefer `requestModelName` (e.g.
/// "GLM-5.1"), falling back to the lowercase `model` id. Each candidate is taken
/// only when present AND non-empty, so a blank/null `requestModelName` does not
/// shadow a valid `model`.
fn record_model(value: &Value) -> Option<String> {
    let provider_data = value.get("providerData")?;
    ["requestModelName", "model"].into_iter().find_map(|key| {
        provider_data
            .get(key)
            .and_then(|m| m.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    })
}

fn call_id(value: &Value) -> Option<String> {
    value
        .get("callId")
        .or_else(|| value.get("id"))
        .and_then(|i| i.as_str())
        .map(String::from)
}

/// Concatenate the `text` of every `content[]` item of the given `item_type`
/// (`input_text` for user turns, `output_text` for assistant turns).
fn collect_text(value: &Value, item_type: &str) -> String {
    let mut out = String::new();
    if let Some(items) = value.get("content").and_then(|c| c.as_array()) {
        for item in items {
            if item.get("type").and_then(|t| t.as_str()) == Some(item_type) {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    out.push_str(text);
                }
            }
        }
    }
    out
}

/// Reasoning text lives in `rawContent[].text` (`reasoning_text` items); some
/// records mirror it under `content[]`, so fall back to that.
fn reasoning_text(value: &Value) -> String {
    for key in ["rawContent", "content"] {
        if let Some(items) = value.get(key).and_then(|c| c.as_array()) {
            let mut out = String::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    out.push_str(text);
                }
            }
            if !out.trim().is_empty() {
                return out;
            }
        }
    }
    String::new()
}

/// Map CodeBuddy's `providerData.rawUsage` (OpenAI completions shape) onto
/// `TurnUsage`. `prompt_tokens` already includes the cached prefix, so subtract
/// `cached_tokens` to get the non-cached input.
fn usage_from_raw(value: &Value) -> Option<TurnUsage> {
    let raw = value.get("providerData")?.get("rawUsage")?;
    let prompt = raw
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let completion = raw
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cached = raw
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if prompt == 0 && completion == 0 && cached == 0 {
        return None;
    }
    Some(TurnUsage {
        input_tokens: prompt.saturating_sub(cached),
        output_tokens: completion,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: cached,
    })
}

/// Parse a `function_call`'s `arguments` — a JSON string (or, defensively, an
/// already-decoded object) — into a `Value` for field inspection. Returns `None`
/// for missing/unparseable/non-object arguments.
fn parse_tool_arguments(value: &Value) -> Option<Value> {
    match value.get("arguments")? {
        Value::String(s) => serde_json::from_str::<Value>(s).ok(),
        obj @ Value::Object(_) => Some(obj.clone()),
        _ => None,
    }
}

/// CodeBuddy invokes MCP tools indirectly through its `DeferExecuteTool`
/// virtualization layer (after a `ToolSearch` discovery step), packing the real
/// tool name and parameters into `{ "toolName": "mcp__…__delegate_to_agent",
/// "params": { … } }`. When a tool call's parsed `arguments` carry that wrapper,
/// return the inner `toolName` so the call resolves to its real identity — and
/// renders the dedicated delegation/question card via the existing
/// `normalizeToolName` suffix rules — instead of the opaque `DeferExecuteTool`
/// shell. The `params` wrapper is deliberately left on `input_preview`: the
/// frontend cards (`findDelegationArgs`, `findTaskId`) peel it themselves, and
/// keeping it also stops the live `inferFromInput` from misclassifying
/// `cancel_delegation`'s `{task_id}` as a generic task. Shared with the live ACP
/// path in `acp/connection.rs`.
pub(crate) fn deferred_tool_name(arguments: &Value) -> Option<&str> {
    let obj = arguments.as_object()?;
    obj.get("params")?;
    obj.get("toolName")
        .and_then(|n| n.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// True when the parsed `arguments` carry a non-empty string `subagent_type` —
/// the agent-agnostic sub-agent delegation marker (also used by
/// `acp/connection.rs:is_subagent_invocation` and the frontend `inferFromInput`).
fn declares_subagent(arguments: &Value) -> bool {
    arguments
        .get("subagent_type")
        .and_then(|s| s.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// Resolve a tool call's display name from its `arguments`:
///   1. a `DeferExecuteTool` wrapper unwraps to its inner `toolName`;
///   2. a native call carrying `subagent_type` is renamed to "Agent" (so the
///      renderer routes it into `AgentToolCallPart`, mirroring the OpenCode
///      `parsers/opencode.rs` and Codex `parsers/codex.rs` parsers);
///   3. otherwise the literal `name` is kept.
fn resolve_tool_call_name(value: &Value) -> String {
    if let Some(arguments) = parse_tool_arguments(value) {
        if let Some(inner) = deferred_tool_name(&arguments) {
            return inner.to_string();
        }
        if declares_subagent(&arguments) {
            return "Agent".to_string();
        }
    }
    value
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("unknown")
        .to_string()
}

/// `function_call.arguments` is a JSON string (or, defensively, an object).
fn tool_input_preview(value: &Value) -> Option<String> {
    let arguments = value.get("arguments")?;
    if let Some(s) = arguments.as_str() {
        (!s.is_empty()).then(|| s.to_string())
    } else if arguments.is_object() || arguments.is_array() {
        serde_json::to_string(arguments).ok()
    } else {
        None
    }
}

/// Rebuild the MCP `CallToolResult` envelope from CodeBuddy's
/// `providerData.toolResult.mcpMeta.structuredContent`. Deferred MCP tools
/// (`DeferExecuteTool`) carry their real structured result here, while
/// `output.text` is only the human-readable ack line; surfacing the envelope the
/// frontend delegation/question cards parse (`parseToolOutput` /
/// `parseStatusReport` / `parseAskQuestionOutcome`) lets them recover
/// `child_conversation_id`, status, tasks, and selections. Returns `None` for
/// plain tools (no `mcpMeta`) or MCP tools that return no structured content, so
/// those fall through to the normal text path.
fn deferred_result_envelope(value: &Value) -> Option<String> {
    let tool_result = value.get("providerData")?.get("toolResult")?;
    let mcp_meta = tool_result.get("mcpMeta")?;
    let structured = mcp_meta.get("structuredContent")?;
    if structured.is_null() {
        return None;
    }
    let text = value
        .get("output")
        .and_then(|o| {
            o.get("text")
                .and_then(|t| t.as_str())
                .or_else(|| o.as_str())
        })
        .or_else(|| tool_result.get("content").and_then(|c| c.as_str()))
        .unwrap_or("");
    let is_error = mcp_meta
        .get("isError")
        .and_then(|e| e.as_bool())
        .unwrap_or(false);
    serde_json::to_string(&serde_json::json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": structured,
        "isError": is_error,
    }))
    .ok()
}

/// `function_call_result.output` is `{type:"text", text}`; fall back to the raw
/// string or `providerData.toolResult.content`. Deferred MCP tools first surface
/// their structured `mcpMeta` envelope (see `deferred_result_envelope`).
fn tool_output_preview(value: &Value) -> Option<String> {
    if let Some(envelope) = deferred_result_envelope(value) {
        return Some(envelope);
    }
    if let Some(output) = value.get("output") {
        if let Some(text) = output.as_str() {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        } else if let Some(text) = output.get("text").and_then(|t| t.as_str()) {
            return Some(text.to_string());
        }
    }
    let content = value
        .get("providerData")?
        .get("toolResult")?
        .get("content")?;
    if let Some(text) = content.as_str() {
        Some(text.to_string())
    } else {
        serde_json::to_string(content).ok()
    }
}

/// A tool call failed when `providerData.toolResult.error` is set (CodeBuddy
/// reports tool failures here even while `status` stays "completed"), the
/// status is a failure, or the output text begins with "Error:".
fn tool_is_error(value: &Value) -> bool {
    if let Some(error) = value
        .get("providerData")
        .and_then(|p| p.get("toolResult"))
        .and_then(|tr| tr.get("error"))
    {
        match error {
            Value::Null => {}
            Value::String(s) => {
                if !s.trim().is_empty() {
                    return true;
                }
            }
            _ => return true,
        }
    }

    if let Some(status) = value.get("status").and_then(|s| s.as_str()) {
        if matches!(
            status.trim().to_ascii_lowercase().as_str(),
            "error" | "failed" | "failure" | "cancelled" | "canceled"
        ) {
            return true;
        }
    }

    value
        .get("output")
        .and_then(|o| o.get("text"))
        .and_then(|t| t.as_str())
        .and_then(|t| t.trim_start().get(..6).map(str::to_string))
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("error:"))
}

/// The sub-agent transcript id CodeBuddy records on an `Agent` tool result
/// (`providerData.toolResult.subAgent.sessionId`, e.g. `"agent-cdd7c1ea"`). The
/// transcript lives at `<session_dir>/subagents/<id>.jsonl` — Claude Code's
/// directory layout. Ordinary tool results carry no `subAgent` block, so this
/// returns `None` for them.
fn subagent_transcript_id(result: &Value) -> Option<&str> {
    result
        .get("providerData")?
        .get("toolResult")?
        .get("subAgent")?
        .get("sessionId")?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Parse a CodeBuddy sub-agent transcript and extract its tool calls.
///
/// The sub-agent JSONL uses the same OpenAI-items schema as the main session
/// (`function_call` / `function_call_result` records); we pair them by `callId`
/// and reuse the outer parser's name/preview/error helpers so nested calls
/// render identically to top-level ones. Mirrors `claude.rs`'s
/// `parse_subagent_tool_calls`, which does the same for Claude's schema.
fn parse_codebuddy_subagent_tool_calls(path: &Path) -> Vec<AgentToolCall> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    let reader = BufReader::new(file);

    // (callId, name, input) in encounter order, paired against results by callId.
    let mut calls: Vec<(Option<String>, String, Option<String>)> = Vec::new();
    let mut results: std::collections::HashMap<String, (Option<String>, bool)> =
        std::collections::HashMap::new();

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match value.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "function_call" => {
                calls.push((
                    call_id(&value),
                    resolve_tool_call_name(&value),
                    tool_input_preview(&value).map(|s| truncate_str(&s, 500)),
                ));
            }
            "function_call_result" => {
                if let Some(id) = call_id(&value) {
                    let output = tool_output_preview(&value).map(|s| truncate_str(&s, 500));
                    results.insert(id, (output, tool_is_error(&value)));
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

/// Build `agent_stats` for an `Agent` `function_call_result` by loading the
/// sub-agent's own transcript and extracting its nested tool calls — the
/// historical mirror of the live path, which synthesizes the same `agent_stats`
/// from the streamed child tool calls (`conversation-runtime-context.tsx`).
///
/// Returns `None` for ordinary results (no `subAgent` linkage), a
/// missing/empty transcript, or a sub-agent that ran no tools, so the common
/// case stays a plain tool result. `main_session_path` is the real `.jsonl`
/// path the parser is reading; the transcript sits beside it under
/// `<session_dir>/subagents/`.
fn agent_stats_from_subagent(
    result: &Value,
    main_session_path: &Path,
) -> Option<AgentExecutionStats> {
    let id = subagent_transcript_id(result)?;
    // Path-traversal guard: `id` becomes a filename under the session dir, so it
    // must be a single plain component (rejects separators, `..`, a Windows
    // drive colon, and NUL). See `is_safe_subagent_id`.
    if !is_safe_subagent_id(id) {
        return None;
    }
    let transcript = main_session_path
        .with_extension("")
        .join("subagents")
        .join(format!("{id}.jsonl"));
    if !transcript.exists() {
        return None;
    }
    let tool_calls = parse_codebuddy_subagent_tool_calls(&transcript);
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

fn text_message(
    id: String,
    role: MessageRole,
    text: String,
    ts: DateTime<Utc>,
    usage: Option<TurnUsage>,
    model: Option<String>,
) -> UnifiedMessage {
    UnifiedMessage {
        id,
        role,
        content: vec![ContentBlock::Text { text }],
        timestamp: ts,
        usage,
        duration_ms: None,
        model,
        completed_at: Some(ts),
    }
}

/// Group the flat, chronologically-ordered `UnifiedMessage`s into `MessageTurn`s:
/// User/System messages each become their own turn; an Assistant message starts
/// a turn that absorbs the immediately-following Tool messages (its tool
/// results), stopping at the next Assistant message to keep turns small for
/// virtualization.
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

