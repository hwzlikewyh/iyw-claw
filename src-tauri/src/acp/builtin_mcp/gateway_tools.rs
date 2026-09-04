use serde_json::{json, Value};

use super::tool_identity::{
    CAPABILITY_ID_MAX_CHARS, IMAGE_TOOL, INVOKE_TOOL, KNOWLEDGE_TOOL, MEMORY_TOOL, READ_TOOL,
    SEARCH_TOOL,
};

pub(super) fn values() -> [Value; 6] {
    [
        search_tool(),
        read_tool(),
        invoke_tool(),
        image_tool(),
        knowledge_tool(),
        memory_tool(),
    ]
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
        "description": "Call this gateway role only through the exact current callable identity and surface that advertised it. On an unknown, unsupported, or not-found routing error, stop this gateway for the turn and never retry through another name or surface. Read the full description and current input schema for one exact stable capability id returned by this session's search. This is metadata/schema only: it does not execute the capability or load the current-turn memory policy. For a memory operation, read the policy capability schema, then invoke iyw.memory.policy.read.v1 through invoke_iyw_capability with empty arguments before invoking any other memory capability. Read before invoking and obey the returned schema. Ask for missing referenced objects or required inputs; never guess ids, paths, URLs, field names, or arguments.",
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

fn image_tool() -> Value {
    json!({
        "name": IMAGE_TOOL,
        "description": "Generate or edit an IYW image in one call. Use type=auto when the Agent should choose the shortest route: no input image uses ordinary generation, one image uses variation, multiple images use mix, explicit series uses extend, and explicit specialized work uses the named type. HTTPS image URLs are submitted directly; Data URLs, raw base64, and workspace-local files are uploaded by the host without checkImage. The host waits for a terminal result and returns public result URLs. Put every operation-specific parameter under parameters; do not invent endpoint names or retry a charged task.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "type": {
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
                },
                "prompt": {"type": "string", "maxLength": 12000},
                "images": {
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
                },
                "parameters": {"type": "object", "additionalProperties": true},
                "wait": {
                    "type": "object",
                    "properties": {
                        "timeoutSeconds": {"type": "integer", "minimum": 0, "maximum": 600, "default": 180},
                        "pollIntervalSeconds": {"type": "number", "exclusiveMinimum": 0, "maximum": 30, "default": 2}
                    },
                    "additionalProperties": false
                },
                "delivery": {
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
                }
            },
            "additionalProperties": false
        }
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
    json!({
        "name": MEMORY_TOOL,
        "description": "Operate the host-owned IYW memory group in one stable tool. The host performs the current-turn policy preflight automatically for operations other than policy.read. Use only the listed operation and pass its complete operation-specific fields under parameters; permissions, scopes, revisions, eTags, candidate lifecycle and preview gates remain host-owned.",
        "inputSchema": {
            "type": "object",
            "required": ["operation"],
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": [
                        "policy.read", "recall", "documents.read", "append", "propose",
                        "candidates.list", "candidate.resolve", "candidate.delete",
                        "harvest.status", "harvest.rescan", "candidate.index.rebuild",
                        "settings.read", "documents.update", "documents.correct"
                    ]
                },
                "parameters": {"type": "object", "additionalProperties": true}
            },
            "additionalProperties": false
        }
    })
}
