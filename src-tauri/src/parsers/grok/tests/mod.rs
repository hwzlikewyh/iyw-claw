use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::*;
use crate::models::{AgentType, ContentBlock, TurnRole};

mod metadata;
mod tools;

pub(super) const SESSION_ID: &str = "019f45e3-e1ef-7690-a29f-fe2554382b49";
pub(super) const SUMMARY: &str = r#"{
    "info": {"id": "019f45e3-e1ef-7690-a29f-fe2554382b49", "cwd": "/Users/me/proj"},
    "session_summary": "Fallback summary",
    "generated_title": "Build the project",
    "created_at": "2026-07-09T07:59:50.598122Z",
    "updated_at": "2026-07-09T08:02:09.789572Z",
    "num_messages": 6,
    "current_model_id": "grok-4.5",
    "head_branch": "main"
}"#;

pub(super) const UPDATES: &str = concat!(
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"你会做什么"},"_meta":{"modelId":"grok-4.5","promptIndex":0}}},"timestamp":1783584019}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"Thinking about it"}}},"timestamp":1783584019}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"我是 Grok"}}},"timestamp":1783584024}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"turn_completed","prompt_id":"p0","stop_reason":"end_turn"}},"timestamp":1783584024}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"执行 pnpm build"},"_meta":{"promptIndex":1}}},"timestamp":1783584029}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"正在执行"}}},"timestamp":1783584029}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call","toolCallId":"call-1","title":"run_terminal_command","rawInput":{"command":"pnpm build"},"_meta":{"x.ai/tool":{"name":"run_terminal_command","kind":"execute"}}}},"timestamp":1783584029}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"call-1","status":"in_progress","content":[{"type":"content","content":{"type":"text","text":"partial output"}}]}},"timestamp":1783584033}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"task_completed","task_snapshot":{"task_id":"call-1","output":"build ok","exit_code":0}}},"timestamp":1783584122}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"call-1","status":"in_progress","content":[{"type":"content","content":{"type":"text","text":"STALE trailing output"}}]}},"timestamp":1783584123}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"turn_completed","prompt_id":"p1","stop_reason":"end_turn"}},"timestamp":1783584129}"#,
    "\n",
);

pub(super) fn fixture(summary: &str, updates: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("tempdir");
    let sessions = temp.path().join("sessions");
    let session = sessions.join("%2FUsers%2Fme%2Fproj").join(SESSION_ID);
    fs::create_dir_all(&session).expect("session directory");
    write(&session, "summary.json", summary);
    write(&session, "updates.jsonl", updates);
    (temp, sessions)
}

pub(super) fn detail(updates: &str) -> crate::models::ConversationDetail {
    let (_temp, sessions) = fixture(SUMMARY, updates);
    GrokParser::with_base_dir(sessions)
        .get_conversation(SESSION_ID)
        .expect("conversation")
}

fn write(directory: &Path, name: &str, contents: &str) {
    let mut file = fs::File::create(directory.join(name)).expect("fixture file");
    file.write_all(contents.as_bytes())
        .expect("fixture content");
}
