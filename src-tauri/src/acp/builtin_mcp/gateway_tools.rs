use serde_json::{json, Value};

use super::tool_identity::{
    ARTIFACTS_TOOL, CAPABILITY_ID_MAX_CHARS, INVOKE_TOOL, KNOWLEDGE_TOOL, MEMORY_TOOL, READ_TOOL,
    SEARCH_TOOL,
};

pub(super) const MEMORY_CAPABILITIES: [(&str, &str); 17] = [
    ("maintenance.read", "iyw.memory.maintenance.read.v1"),
    ("review.resolve", "iyw.memory.review.resolve.v1"),
    ("policy.read", "iyw.memory.policy.read.v1"),
    ("recall", "iyw.memory.recall.search.v1"),
    ("retire", "iyw.memory.retire.v1"),
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
}

fn search_tool() -> Value {
    json!({
        "name": SEARCH_TOOL,
        "description": "Discover local host and signed-in remote capabilities. The attached remote overview and top-level tools come from the live server and refresh every 15 minutes; capability categories are not fixed. For a capability introduction or unknown scope use source=remote, mode=browse, omit query and follow next_cursor until null. To browse a group's members pass its returned capability_id as group_id; keep it unchanged while paging. For a concrete task use mode=search with focused query keywords: source=local for host work, remote for remote business, all when unsure. Prefer an advertised direct tool with a complete matching schema. Results are metadata, not business data: read a plausible capability_id and invoke a member, never a group. A full group read includes member schemas and usage; reuse definitions. Remote unavailable/degraded or empty search does not prove absence; browse needs no semantic index. Do not discover tools for greetings or unrelated local work. Never guess IDs or callable namespaces.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string", "minLength": 1, "maxLength": 256, "description": "Required in search mode: focused action/object keywords. Omit in browse mode. Search capabilities here; use search_iyw_knowledge for document content."},
                "mode": {"type": "string", "enum": ["search", "browse"], "default": "search", "description": "browse requires source=remote and no query; use for capability introductions or unknown scope."},
                "group_id": {"type": "string", "minLength": 1, "maxLength": CAPABILITY_ID_MAX_CHARS, "description": "Browse only: exact opaque capability_id of a returned remote group. Never supply the raw remote ID."},
                "cursor": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Browse only: copy next_cursor from the previous page with the same group_id. Account/catalog changes may require restarting browse."},
                "limit": {"type": "integer", "minimum": 1, "maximum": 20, "default": 8, "description": "Maximum candidates per selected source; all may return up to twice this number. Prefer a small precise result set."},
                "source": {"type": "string", "enum": ["all", "local", "remote"], "default": "all", "description": "local never waits for the network; remote discovers signed-in business capabilities; all searches both."}
            },
            "additionalProperties": false
        }
    })
}

fn read_tool() -> Value {
    json!({
        "name": READ_TOOL,
        "description": "Read the full instructions/schema for an opaque capability_id copied from search, a group member or an advertised memory mapping. Direct tools already carry their definitions and need no read; no image-generation capability_id exists. Remote group description gives the workflow and items contain member capability_id, input_schema and usage including use_when, argument_sources and result_summary. Read all relevant nested constraints and obtain required business IDs from documented prerequisites or the user. A member fully described in this result needs no additional read. Reuse definitions through the conversation; a remote TOOL_CHANGED with execution_status=not_started requires rereading the old capability_id and using the current ID returned here. The host supplies remote versions. remote_catalog_expired requires one fresh search. Metadata reads execute no business operation and never authorize replaying an uncertain call. Memory invoked through the catalog still needs policy preflight; manage_iyw_memory handles it automatically. Unknown callable identity ends this route; do not guess another namespace.",
        "inputSchema": {
            "type": "object",
            "required": ["capability_id"],
            "properties": {"capability_id": {
                "type": "string", "minLength": 1,
                "maxLength": CAPABILITY_ID_MAX_CHARS,
                "description": "Copy an opaque capability_id exactly from this session's search/group result, current remote overview or advertised memory mapping. Never derive an ID from a tool name, add .v1, or put a direct tool's name here."
            }},
            "additionalProperties": false
        }
    })
}

fn invoke_tool() -> Value {
    json!({
        "name": INVOKE_TOOL,
        "description": "Execute a local or remote catalog capability by its exact returned capability_id and schema-matching arguments. Before first use, read its full description/input_schema or the full member definition in a group read; reuse it thereafter. Groups are not invocable. The host routes and supplies remote tool_id/tool_version; never put these routing fields in business arguments. Preserve declared types, enums, prerequisites, authorization and parameter sources. Inspect MCP isError and business status/data; queued is not complete. A capability_schema_mismatch or remote INVALID_ARGUMENTS with execution_status=not_started allows one correction from the read schema and field hints. Remote TOOL_CHANGED with not_started requires rereading the old capability_id and using the returned current member ID. remote_catalog_expired allows fresh discovery before an unstarted operation. Never automatically repeat writes after timeout, cancellation or unknown outcome: check original task/idempotency evidence. A backend failure is not a renamed tool. Unknown callable identity ends this route. Echo an earlier iyw_delivery_receipt only as top-level delivery_ack on the next real invocation, never inside arguments or in a fabricated acknowledgement call.",
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
        "description": "One entry for memory recall, learning, correction and maintenance. Common operations use inline parameters; others require one read of their mapped capability schema. Reuse relevant memory, append explicit durable user facts, propose uncertain observations, and retire only exact entries disproved by current evidence. Handle routine candidates and reviews during related work without asking the user to maintain queues. Never treat model suggestions, age or low usage as proof; never store secrets, sensitive inferences or transient tasks. Writes require successful receipts; revision conflicts require fresh reads. Retirement preserves history. Unavailable memory must not block the task or trigger direct file edits. Detailed workflows, limits and examples: iyw-capability-gateway/references/memory-and-learning.md and memory-examples.md.",
        "inputSchema": {
            "type": "object",
            "required": ["operation"],
            "examples": [{"operation": "recall", "parameters": {"query": "prior project decisions", "limit": 3}}],
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": operations,
                    "description": format!("recall, append, propose, retire and documents.read use the inline parameters below. For other operations only, read the matching capability_id once with read_iyw_capability: {read_targets}")
                },
                "parameters": super::iyw_memory::parameters_schema()
            },
            "additionalProperties": false
        }
    })
}
