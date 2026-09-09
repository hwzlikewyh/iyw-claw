use serde_json::{json, Value};

use super::tool_identity::{
    ARTIFACTS_TOOL, CAPABILITY_ID_MAX_CHARS, INVOKE_TOOL, KNOWLEDGE_TOOL, MEMORY_TOOL, READ_TOOL,
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

pub(super) fn values() -> [Value; 12] {
    [
        super::interaction_tools::embedded_tool(super::interaction_tools::ASK_TOOL),
        super::interaction_tools::html_tool(),
        super::interaction_tools::embedded_tool(ARTIFACTS_TOOL),
        search_tool(),
        read_tool(),
        invoke_tool(),
        super::iyw_image_schema::tool(),
        super::iyw_image_models::tool(),
        super::iyw_fetch::tool(),
        super::iyw_upload::tool(),
        knowledge_tool(),
        memory_tool(),
    ]
    .map(with_usage_instruction)
}

fn with_usage_instruction(mut tool: Value) -> Value {
    let description = tool["description"].as_str().unwrap_or_default();
    tool["description"] = json!(format!(
        "Read this advertised definition, including nested schema fields and examples, before first use; reuse it throughout the conversation. For a direct tool, reading this definition requires no extra discovery or metadata call unless its instructions require a model lookup or operation-specific capability read. {description}"
    ));
    tool
}

fn search_tool() -> Value {
    json!({
        "name": SEARCH_TOOL,
        "description": "Discover host capabilities for a concrete subgoal not covered by a directly advertised tool. Prefer direct image, knowledge, memory, question, HTML, and artifact tools whenever they cover the subgoal, even without an explicit user tool choice. Use this catalog for remaining session, profile, history, browser/media, channel, automation, or delegation work. Reuse a matching capability already discovered and fully read in this session; do not search again for every invocation or schema error. For a new capability, search before claiming it unavailable. Use two to five Chinese or English action/object terms, then read the best available match; search summaries are not executable schemas. Ask for an unknown required target before invoking; never guess IDs. Do not enumerate capabilities for greetings or self-contained work. Read at most two plausible candidates; an exhausted result set permits one search with a close synonym. Use only the current advertised gateway identity. An unknown, unsupported, or not-found gateway route ends this gateway attempt; do not switch names or surfaces.",
        "inputSchema": {
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string", "minLength": 1, "maxLength": 256, "description": "Two to five action/object keywords for the needed host capability, for example list scheduled tasks. Search capabilities here; use search_iyw_knowledge for document content."},
                "limit": {"type": "integer", "minimum": 1, "maximum": 20, "default": 8, "description": "Maximum candidates returned. Use a small limit for a precise query; omit for the default eight."}
            },
            "additionalProperties": false
        }
    })
}

fn read_tool() -> Value {
    json!({
        "name": READ_TOOL,
        "description": "Read metadata only for a catalog capability. Copy the exact capability_id from this session's search result or the advertised manage_iyw_memory operation mapping; treat it as opaque, not a name to invent or derive from a tool. Direct tools such as generate_iyw_image, search_iyw_knowledge, and show_interactive_html carry their schemas in their advertised definitions and do not need this read. There is no registered image-generation capability_id. Read the full returned description and nested input_schema before constructing arguments, then reuse it throughout the conversation. Search summaries are insufficient. Reading metadata does not execute an operation, repair a backend rejection, or authorize replaying an earlier request. For memory via invoke_iyw_capability, execute the current-turn policy preflight; manage_iyw_memory handles it automatically. On capability_not_found, use the returned guidance to identify the correct surface and stop guessed-ID retries. If this gateway's callable identity itself is unknown, stop this server route without trying alternate names.",
        "inputSchema": {
            "type": "object",
            "required": ["capability_id"],
            "properties": {"capability_id": {
                "type": "string", "minLength": 1,
                "maxLength": CAPABILITY_ID_MAX_CHARS,
                "description": "Copy an opaque capability_id exactly from this session's search result or advertised memory operation mapping. Never derive an ID from a tool name, add .v1, or put a direct tool's name here."
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
                    "maxLength": CAPABILITY_ID_MAX_CHARS,
                    "description": "Exact opaque ID from this session's catalog search, whose full schema you have read. A direct tool name or an ID inferred from naming conventions is invalid."},
                "arguments": {"type": "object", "description": "JSON object matching the capability's input_schema exactly. Put its business fields directly here; do not send a JSON string, copy the schema, or add an extra parameters/arguments wrapper unless that schema declares one."},
                "delivery_ack": {"type": "string", "minLength": 1, "maxLength": 128}
            },
            "additionalProperties": false
        }
    })
}


fn knowledge_tool() -> Value {
    json!({
        "name": KNOWLEDGE_TOOL,
        "description": "Search IYW document knowledge directly; no capability search/read or image task is needed. Supply one focused query and omit optional filters unless their exact IDs or category are known. Request only enough results for the question. Returns count and document snippets with safe metadata; reuse relevant results while completing the same subgoal. This searches document content, not tool definitions, account identity, or personal memory.",
        "inputSchema": {
            "type": "object",
            "required": ["query"],
            "examples": [{"query": "product design requirements", "limit": 5}],
            "properties": {
                "query": {"type": "string", "minLength": 1, "maxLength": 4096, "description": "The focused question or concepts to retrieve; do not paste the entire conversation."},
                "category": {"type": "integer", "default": 0, "description": "Known numeric category code. Omit to use the default; do not invent category names or codes."},
                "folderId": {"type": ["integer", "null"], "description": "Optional exact numeric folder ID from known context. Omit when unrestricted; not a filesystem path."},
                "fileId": {"type": ["string", "null"], "description": "Optional exact document ID as a string, including numeric-looking IDs. Omit when unrestricted; not an integer or a file path."},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 10, "description": "Maximum results requested; prefer the smallest useful set."},
                "denseWeight": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.5, "description": "Retrieval weighting. Omit to keep the default 0.5 unless the task needs explicit tuning."}
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
        "description": "Operate host-owned IYW memory directly. No capability search is needed: read the selected operation's full description and input_schema once with read_iyw_capability using operation.description's exact mapping, then reuse that read. Put the operation's business fields under parameters. For prior decisions or preferences, use recall; for authoritative document contents, use documents.read. The host performs current-turn policy preflight automatically, so do not add a separate policy.read execution before this tool. Select policy.read only when the policy itself is needed. Metadata reads do not execute operations. Permissions, scopes, revisions, eTags, candidate lifecycle and preview gates remain host-owned.",
        "inputSchema": {
            "type": "object",
            "required": ["operation"],
            "examples": [{"operation": "recall", "parameters": {"query": "prior project decisions", "limit": 3}}],
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": operations,
                    "description": format!("Before first use, read the matching capability_id with read_iyw_capability; reuse a prior read in this conversation: {read_targets}")
                },
                "parameters": {"type": "object", "additionalProperties": true, "description": "JSON object matching the selected operation's input_schema. Place business fields directly here, without another arguments/parameters wrapper. Omit for operations whose schema requires no inputs. Use only that operation's declared fields and enum values."}
            },
            "additionalProperties": false
        }
    })
}
