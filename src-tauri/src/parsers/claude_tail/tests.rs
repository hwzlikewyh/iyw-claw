use std::path::PathBuf;

use crate::models::{ContentBlock, TurnRole};
use crate::parsers::claude_background::BACKGROUND_TASK_MARKER;

use super::ClaudeTailAccumulator;

#[test]
fn preserves_local_blocks_usage_and_background_lifecycle() {
    let mut accumulator = ClaudeTailAccumulator::new(PathBuf::from("session.jsonl"));
    let records = [
        serde_json::json!({
            "type": "user",
            "uuid": "user-1",
            "timestamp": "2026-07-16T10:00:00Z",
            "cwd": "D:/work",
            "gitBranch": "main",
            "message": {
                "role": "user",
                "content": [
                    { "type": "text", "text": "inspect this" },
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": "image/png",
                            "data": "aW1hZ2U="
                        }
                    }
                ]
            }
        }),
        serde_json::json!({
            "type": "assistant",
            "uuid": "assistant-1",
            "timestamp": "2026-07-16T10:00:01Z",
            "message": {
                "role": "assistant",
                "model": "claude-sonnet-4-6",
                "usage": {
                    "input_tokens": 3,
                    "output_tokens": 5,
                    "cache_creation_input_tokens": 7,
                    "cache_read_input_tokens": 11
                },
                "content": [{
                    "type": "tool_use",
                    "id": "toolu-agent",
                    "name": "Agent",
                    "input": { "prompt": "check" }
                }]
            }
        }),
        serde_json::json!({
            "type": "user",
            "uuid": "ack-1",
            "timestamp": "2026-07-16T10:00:02Z",
            "message": {
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "toolu-agent",
                    "content": "Async agent launched successfully"
                }]
            },
            "toolUseResult": {
                "status": "async_launched",
                "agentId": "agent-1"
            }
        }),
        serde_json::json!({
            "type": "user",
            "uuid": "notification-1",
            "timestamp": "2026-07-16T10:00:03Z",
            "message": {
                "role": "user",
                "content": "<task-notification>\n<task-id>agent-1</task-id>\n<tool-use-id>toolu-agent</tool-use-id>\n<status>completed</status>\n<summary>Done</summary>\n<result>Verified locally</result>\n</task-notification>"
            }
        }),
        serde_json::json!({
            "type": "assistant",
            "uuid": "assistant-2",
            "timestamp": "2026-07-16T10:00:04Z",
            "message": {
                "role": "assistant",
                "model": "claude-sonnet-4-6",
                "content": [{ "type": "text", "text": "finished" }]
            }
        }),
    ];
    for record in records {
        accumulator.feed_value(record);
    }

    let metadata = accumulator.metadata();
    assert_eq!(metadata.cwd.as_deref(), Some("D:/work"));
    assert_eq!(metadata.git_branch.as_deref(), Some("main"));
    assert_eq!(metadata.model.as_deref(), Some("claude-sonnet-4-6"));
    assert_eq!(metadata.title.as_deref(), Some("inspect this"));
    assert_eq!(
        metadata.first_timestamp.unwrap().to_rfc3339(),
        "2026-07-16T10:00:00+00:00"
    );
    assert_eq!(
        metadata.last_timestamp.unwrap().to_rfc3339(),
        "2026-07-16T10:00:04+00:00"
    );

    let turns = accumulator.collect_turns(Some("D:/work"));
    assert_eq!(turns.len(), 3);
    assert!(matches!(turns[0].role, TurnRole::User));
    assert!(turns[0]
        .blocks
        .iter()
        .any(|block| matches!(block, ContentBlock::Image { mime_type, data, .. } if mime_type == "image/png" && data == "aW1hZ2U=")));

    let assistant = &turns[1];
    assert_eq!(assistant.model.as_deref(), Some("claude-sonnet-4-6"));
    let usage = assistant.usage.as_ref().unwrap();
    assert_eq!(usage.input_tokens, 3);
    assert_eq!(usage.output_tokens, 5);
    let marker = assistant
        .blocks
        .iter()
        .find_map(|block| match block {
            ContentBlock::ToolResult {
                output_preview: Some(output),
                ..
            } if output.starts_with(BACKGROUND_TASK_MARKER) => Some(output),
            _ => None,
        })
        .expect("launch result is rewritten to a background marker");
    let payload: serde_json::Value =
        serde_json::from_str(marker.strip_prefix(BACKGROUND_TASK_MARKER).unwrap()).unwrap();
    assert_eq!(payload["task_id"], "agent-1");
    assert_eq!(payload["status"], "completed");
    assert_eq!(payload["result"], "Verified locally");

    assert!(matches!(turns[2].role, TurnRole::Assistant));
    assert!(matches!(
        turns[2].blocks.as_slice(),
        [ContentBlock::Text { text }] if text == "finished"
    ));
}
