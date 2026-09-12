use serde_json::{json, Value};

use super::tool_identity::{
    ARTIFACTS_TOOL, CAPABILITY_ID_MAX_CHARS, IMAGE_TOOL, INVOKE_TOOL, KNOWLEDGE_TOOL, MEMORY_TOOL, READ_TOOL,
    SEARCH_TOOL,
};

pub(super) const MEMORY_CAPABILITIES: [(&str, &str); 15] = [
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

pub(super) fn values() -> [Value; 11] {
    [
        super::interaction_tools::embedded_tool(super::interaction_tools::ASK_TOOL),
        super::interaction_tools::html_tool(),
        super::interaction_tools::embedded_tool(ARTIFACTS_TOOL),
        search_tool(),
        read_tool(),
        invoke_tool(),
        image_tool(),
        super::iyw_image_models::tool(),
        super::iyw_fetch::tool(),
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

fn image_tool() -> Value {
    json!({
        "name": IMAGE_TOOL,
        "description": "Generate or edit images using the route that matches the task. Text-to-image has priority on Fusion type=generate (images/generations), including auto without source images. Fusion type=edit (images/edits) is available when explicitly selected and requires at least one source image. Neither requires a prior platform attempt or failure. For either Fusion operation, first call list_iyw_image_models, select a model supporting the operation, and pass its exact id in parameters.model; the host never chooses a default model. Reuse the catalog for the same task or batch. For one-image redesign or color/material/detail changes, prefer platform variation. For combining 2-10 references, prefer mix. For four-panel grids or same-series extension from one reference, prefer extend; keep one base image and describe the grid/series in prompt. With no base image, generate the requested composition using generate. Platform operations need no Fusion model lookup; fission remains available when explicitly selected. Use a matching specialized operation for outpaint, background, super-resolution and similar tasks. Default timeouts are 600 seconds for platform requests/polling and 300 seconds for Fusion; wait.timeoutSeconds may override either, including above 600. This direct tool needs no capability search/read/invoke, separate upload, or another image Skill. Use single-task fields for one task or requests for up to eight independent tasks. count starts multiple charged executions; never combine it with parameters.n or parameters.batchSize. Batches validate before execution, continue after runtime item failures, and return partial results in input order. Register only final deliverables; for intermediate images used in a PPT, report or webpage, set delivery.registerArtifact=false. Deliver ordinary successful images using returned status, URLs and delivery metadata. Inspect visuals for requested review, comparison, visual acceptance or integration into a composed deliverable. Report partial or failed status honestly and do not regenerate beyond scope. Explain the selected operation/backend when asked; the tool name alone does not prove routing. Preserve backend rejection details without inventing capability IDs. A timeout, transport error or non-terminal result is not confirmed generation failure: query the original task_id when available, and never blindly resubmit or switch routes after uncertain submission.",
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
        "prompt": {"type": "string", "maxLength": 12000, "description": "State what to preserve and change, the intended layout, and each reference image's role in input order. Select the operation with type; the prompt alone does not select specialized tools."},
        "images": image_sources_schema(),
        "parameters": {"type": "object", "additionalProperties": true, "description": "Only fields supported by the selected operation. For generate, auto without images, or explicit edit, call list_iyw_image_models and set model to the exact selected id: generate requires capabilities.image_generation=true, edit requires capabilities.image_editing=true. Choose from returned descriptions, capabilities, prices and user requirements. No prior platform failure is required. Missing model IDs and display names are rejected; no default model is selected. Other Fusion options must be supported by the model. variation, extend and mix need only prompt and images for a default result; their toolName and modelChannel are host-owned. Do not copy parameters across operations."},
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
        "default": "auto",
        "description": "Prefer an explicit type matching the task: generate for text-to-image through Fusion images/generations; variation for modifying one source image; mix for fusing 2-10 references in input order; extend for four-panel grids or same-series extension from one base image. Explicit edit uses Fusion images/edits and requires source images. generate/edit need a selected model ID from list_iyw_image_models, with no prior platform attempt or failure. fission and matching specialized platform types remain explicitly available. auto uses generate with no images, extend with one image and series/extension/four-panel/2x2 wording, variation for other single-image prompts, and mix for multiple images. A grid request without a reference uses generate; do not drop extra references to force extend."
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
            "timeoutSeconds": {
                "type": "integer", "minimum": 0,
                "description": "Agent-controlled timeout override in seconds; values above 600 are supported. Omit for 600 seconds on IYW platform requests and task polling, or 300 seconds on generate (images/generations) and edit (images/edits). Positive values override the operation's HTTP timeout and platform polling wait, including per-item waits in a batch. Prefer the defaults or longer for slow image tasks; avoid premature termination. 0 submits platform tasks without polling; generate/edit still use their default HTTP timeout."
            },
            "pollIntervalSeconds": {"type": "number", "exclusiveMinimum": 0, "maximum": 30, "default": 2}
        },
        "additionalProperties": false
    })
}

fn delivery_schema() -> Value {
    json!({
        "type": "object",
        "description": "Register only images that are themselves final deliverables. For intermediate images used inside a PPT, report, webpage or other composed deliverable, set registerArtifact=false and register only the completed deliverable later. No separate user request is needed to identify the final deliverable from the task.",
        "properties": {
            "display": {
                "type": "boolean",
                "default": false,
                "description": "Compatibility option. Artifact registration is controlled by registerArtifact. Intermediate images with registerArtifact=false remain available through result URLs without being registered as deliverables."
            },
            "registerArtifact": {
                "type": "boolean", "default": true,
                "description": "Set false for intermediate assets, drafts and embedded document illustrations, including images generated for a PPT. Leave true only when the images themselves are final deliverables."
            }
        },
        "additionalProperties": false
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
        "description": "Read, retain or retire relevant memory without leaving the task. Call recall, append, propose, retire or documents.read directly using the inline schemas: no search, metadata, Skill or policy read is required. Recall before decisions depending on prior preferences, repeated workflows or failures; reuse relevant results already supplied. Append only when the user explicitly asks to remember a durable fact or preference; otherwise propose reusable user signals without asking for approval. A proposal is not confirmed memory. Retire a recalled obsolete entry or experience using its id as memoryId, sourceRevision as expectedRevision, and an evidence-based reason. Omit expiresAt to forget immediately; set a known RFC3339 expiry only from evidence. Never expire stable preferences just because they are old. Retirement excludes recall but preserves source history; stop applying the old fact in this conversation too. Never store secrets, sensitive inferences, repository facts, temporary progress or Agent reflections as user memory. Documents.read returns raw authoritative text for editing plus inactiveEntryIds: those entries must not inform decisions. Other operations require one read of their mapped capability schema. The host performs policy, authorization, scope and concurrency checks. matched is evidence, no_evidence is no match, unavailable is not absence. Memory failure must not block the task or trigger file edits.",
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
