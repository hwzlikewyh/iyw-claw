use serde_json::{json, Value};

use super::tool_identity::{
    ARTIFACTS_TOOL, CAPABILITY_ID_MAX_CHARS, IMAGE_TOOL, INVOKE_TOOL, KNOWLEDGE_TOOL, MEMORY_TOOL, READ_TOOL,
    SEARCH_TOOL,
};

pub(super) const MEMORY_CAPABILITIES: [(&str, &str); 14] = [
    ("policy.read", "iyw.memory.policy.read.v1"),
    ("recall", "iyw.memory.recall.search.v1"),
    ("documents.read", "iyw.memory.documents.read.v1"),
    ("append", "iyw.memory.confirmed.append.v1"),
    ("propose", "iyw.memory.candidate.propose.v1"),
    ("candidates.list", "iyw.memory.candidates.list.v1"),
    ("candidate.resolve", "iyw.memory.candidate.resolve.v1"),
    ("candidate.delete", "iyw.memory.candidate.delete.v1"),
    ("harvest.status", "iyw.memory.harvest.status.v1"),
    ("harvest.rescan", "iyw.memory.harvest.rescan.v1"),
    (
        "candidate.index.rebuild",
        "iyw.memory.candidate.index.rebuild.v1",
    ),
    ("settings.read", "iyw.memory.settings.read.v1"),
    ("documents.update", "iyw.memory.documents.update.v1"),
    ("documents.correct", "iyw.memory.documents.correct.v1"),
];

pub(super) fn values() -> [Value; 9] {
    [
        super::interaction_tools::embedded_tool(super::interaction_tools::ASK_TOOL),
        super::interaction_tools::html_tool(),
        super::interaction_tools::embedded_tool(ARTIFACTS_TOOL),
        search_tool(),
        read_tool(),
        invoke_tool(),
        image_tool(),
        knowledge_tool(),
        memory_tool(),
    ]
    .map(with_usage_instruction)
}

fn with_usage_instruction(mut tool: Value) -> Value {
    let description = tool["description"].as_str().unwrap_or_default();
    tool["description"] = json!(format!(
        "You must read this tool's full usage description and input schema, including nested fields, constraints and examples, before first use. If already read in this conversation, reuse it without another read. {description}"
    ));
    tool
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
        "description": "Call this gateway role only through the exact current callable identity and surface that advertised it. On an unknown, unsupported, or not-found routing error, stop this gateway for the turn and never retry through another name or surface. Before first using a capability, read the full usage description and input schema for its exact stable id returned by this session's search or manage_iyw_memory operation mapping. Read the entire result before constructing arguments; search summaries are insufficient. If already read in this conversation, reuse that result without another read, including after an ordinary parameter error. This is an Agent instruction, not a server read gate. This tool reads metadata only and does not execute a capability. For memory through invoke_iyw_capability, separately invoke iyw.memory.policy.read.v1 before other memory capabilities, reading its instructions once before first use. manage_iyw_memory performs that policy execution automatically. Ask for missing referenced objects or required inputs; never guess ids, paths, URLs, field names, or arguments.",
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
        "description": "Call this gateway role only through the exact current callable identity and surface that advertised it. On an unknown, unsupported, or not-found routing error, stop this gateway for the turn and never retry through another name or surface. Invoke an available IYW capability using an exact stable id returned by this session's search. You must call read_iyw_capability and read its full usage description and input schema before first using that capability. If already read in this conversation, reuse the instructions without another read; the host does not track or block calls based on read history. Use only declared fields and enum values; omit unnecessary optional fields and never borrow parameters from another tool. A capability_schema_mismatch with execution_status=not_started permits one correction using the instructions already read and the error's field hints, then retry the same intended operation once. This also applies to text-only schema errors explicitly reporting execution_status=not_started. A parameter error does not require rereading. Stop if that correction fails; never replay unchanged arguments or apply this recovery to unavailable, routing, timeout, permission, or effect-unknown errors. If a prior response returned iyw_delivery_receipt and a later real invocation is needed, echo it only as top-level delivery_ack; never put it in arguments or fabricate an invocation just to acknowledge it.",
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

fn image_tool() -> Value {
    json!({
        "name": IMAGE_TOOL,
        "description": "Generate or edit IYW images in one call. Use the existing single-task fields for one task, or requests for up to eight tasks with independent types, prompts, images, parameters, waits, and counts. count intentionally starts multiple charged executions and is never an automatic retry. A batch is fully validated before execution, continues after runtime item failures, returns partial results in input order, and registers every successful result URL together. Use type=auto for the shortest route. Put operation-specific parameters under parameters; never combine count with parameters.n or parameters.batchSize.",
        "inputSchema": {
            "type": "object",
            "oneOf": [single_image_schema(), batch_image_schema()]
        }
    })
}

fn single_image_schema() -> Value {
    let mut schema = image_request_schema(false);
    schema["properties"]["delivery"] = delivery_schema();
    schema
}

fn batch_image_schema() -> Value {
    json!({
        "type": "object",
        "required": ["requests"],
        "properties": {
            "requests": {
                "type": "array",
                "minItems": 1,
                "maxItems": 8,
                "items": image_request_schema(true)
            },
            "delivery": delivery_schema()
        },
        "additionalProperties": false
    })
}

fn image_request_schema(include_id: bool) -> Value {
    let mut properties = json!({
        "type": image_type_schema(),
        "prompt": {"type": "string", "maxLength": 12000},
        "images": image_sources_schema(),
        "parameters": {"type": "object", "additionalProperties": true},
        "count": {
            "type": "integer",
            "minimum": 1,
            "maximum": 4,
            "default": 1,
            "description": "Intentional execution count. Do not combine with parameters.n or parameters.batchSize."
        },
        "wait": wait_schema()
    });
    if include_id {
        properties["id"] = json!({"type": "string", "minLength": 1, "maxLength": 64});
    }
    json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false
    })
}

fn image_type_schema() -> Value {
    json!({
        "type": "string",
        "enum": [
            "auto", "generate", "edit", "variation", "extend", "mix",
            "fission", "pattern-apply", "free-imitation", "material-product",
            "ip-apply", "outpaint", "super-resolution", "split-layers",
            "separate-layers", "enhance", "extract-pattern", "repeat-horizontal",
            "convert", "line-extraction", "color-transfer", "image-to-3d",
            "video", "model-scene", "background"
        ],
        "default": "auto"
    })
}

fn image_sources_schema() -> Value {
    json!({
        "type": "array",
        "minItems": 0,
        "maxItems": 10,
        "items": {
            "oneOf": [
                {"type": "string", "minLength": 1},
                {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string", "minLength": 1},
                        "path": {"type": "string", "minLength": 1},
                        "base64": {"type": "string", "minLength": 1},
                        "data": {"type": "string", "minLength": 1},
                        "mimeType": {"type": "string", "minLength": 1},
                        "role": {"type": "string", "maxLength": 64},
                        "name": {"type": "string", "maxLength": 255}
                    },
                    "additionalProperties": false
                }
            ]
        }
    })
}

fn wait_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "timeoutSeconds": {"type": "integer", "minimum": 0, "maximum": 600, "default": 180},
            "pollIntervalSeconds": {"type": "number", "exclusiveMinimum": 0, "maximum": 30, "default": 2}
        },
        "additionalProperties": false
    })
}

fn delivery_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "display": {
                "type": "boolean",
                "default": false,
                "description": "Compatibility option. Result URLs are registered directly and are never downloaded by the host."
            },
            "registerArtifact": {"type": "boolean", "default": true}
        },
        "additionalProperties": false
    })
}

fn knowledge_tool() -> Value {
    json!({
        "name": KNOWLEDGE_TOOL,
        "description": "Search the IYW knowledge base as an independent operation. It never starts an image task and returns only bounded document snippets and safe document metadata.",
        "inputSchema": {
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string", "minLength": 1, "maxLength": 4096},
                "category": {"type": "integer", "default": 0},
                "folderId": {"type": ["integer", "null"]},
                "fileId": {"type": ["string", "null"]},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 10},
                "denseWeight": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.5}
            },
            "additionalProperties": false
        }
    })
}

fn memory_tool() -> Value {
    let operations = MEMORY_CAPABILITIES.map(|(operation, _)| operation);
    let read_targets = MEMORY_CAPABILITIES
        .iter()
        .map(|(operation, id)| format!("{operation}={id}"))
        .collect::<Vec<_>>()
        .join("; ");
    json!({
        "name": MEMORY_TOOL,
        "description": "Operate the host-owned IYW memory group in one stable tool. You must read each operation's full description and input_schema with read_iyw_capability before first use; use the exact id in operation.description. If already read in this conversation, reuse it without another read. Put that schema's fields under parameters, then call this tool. Reading instructions is an Agent rule, not a server read gate. The host performs the current-turn memory policy preflight automatically for operations other than policy.read; reading operation instructions does not execute that policy. Permissions, scopes, revisions, eTags, candidate lifecycle and preview gates remain host-owned.",
        "inputSchema": {
            "type": "object",
            "required": ["operation"],
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": operations,
                    "description": format!("Before first use, read the matching capability_id with read_iyw_capability; reuse a prior read in this conversation: {read_targets}")
                },
                "parameters": {"type": "object", "additionalProperties": true}
            },
            "additionalProperties": false
        }
    })
}
