use serde_json::{json, Value};

use super::tool_identity::{CAPABILITY_ID_MAX_CHARS, INVOKE_TOOL, READ_TOOL, SEARCH_TOOL};

pub(super) fn values() -> [Value; 3] {
    [search_tool(), read_tool(), invoke_tool()]
}

fn search_tool() -> Value {
    json!({
        "name": SEARCH_TOOL,
        "description": "Call this gateway role only through the exact current callable identity and surface that advertised it. On an unknown, unsupported, or not-found routing error, stop this gateway for the turn and never retry through another name or surface. Proactively search the current session's IYW capability catalog when a concrete goal needs host-side state or action, especially delegation, submitting feedback or user questions, session state, image or media work, task artifacts, persistent memory, current user profile, channels, or automation. Prior decisions, preferences, repeated workflows, or earlier context make task-scoped memory recall a concrete subgoal. A final user-facing file, directory, or public URL makes Artifact registration a required subgoal before completion. Search once before claiming such a step is unavailable or asking the user to do it manually when no direct tool fits. A user-requested exact visible direct tool takes precedence only for the subgoal it fully satisfies; apply discovery independently to remaining host-side subgoals. Ask for a missing primary object before search. Use two to five discriminating action/object keywords; normalized Chinese and English intent terms are accepted. Do not search greetings, ordinary questions, self-contained trivial tasks, current-turn-only context, every turn, or merely to enumerate capabilities. Read at most two plausible candidates per result set. An empty result, no plausible candidate, or two non-matches permits the single search retry.",
        "inputSchema": {
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string", "minLength": 1, "maxLength": 256},
                "limit": {"type": "integer", "minimum": 1, "maximum": 20, "default": 8}
            },
            "additionalProperties": false
        }
    })
}

fn read_tool() -> Value {
    json!({
        "name": READ_TOOL,
        "description": "Call this gateway role only through the exact current callable identity and surface that advertised it. On an unknown, unsupported, or not-found routing error, stop this gateway for the turn and never retry through another name or surface. Read the full description and current input schema for one exact stable capability id returned by this session's search. Read before invoking and obey the returned schema. Ask for missing referenced objects or required inputs; never guess ids, paths, URLs, field names, or arguments.",
        "inputSchema": {
            "type": "object",
            "required": ["capability_id"],
            "properties": {"capability_id": {
                "type": "string", "minLength": 1,
                "maxLength": CAPABILITY_ID_MAX_CHARS
            }},
            "additionalProperties": false
        }
    })
}

fn invoke_tool() -> Value {
    json!({
        "name": INVOKE_TOOL,
        "description": "Call this gateway role only through the exact current callable identity and surface that advertised it. On an unknown, unsupported, or not-found routing error, stop this gateway for the turn and never retry through another name or surface. Invoke an available IYW capability using an exact stable id returned by this session's search. Supply arguments exactly as described by read_iyw_capability. If the id becomes unavailable or routing fails, do not retry under a guessed id or namespace. If a prior response returned iyw_delivery_receipt and a later real invocation is needed, echo it only as top-level delivery_ack; never put it in arguments or fabricate an invocation just to acknowledge it.",
        "inputSchema": {
            "type": "object",
            "required": ["capability_id", "arguments"],
            "properties": {
                "capability_id": {"type": "string", "minLength": 1,
                    "maxLength": CAPABILITY_ID_MAX_CHARS},
                "arguments": {"type": "object"},
                "delivery_ack": {"type": "string", "minLength": 1, "maxLength": 128}
            },
            "additionalProperties": false
        }
    })
}
