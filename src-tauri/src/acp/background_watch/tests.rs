use std::io::Write;
use std::time::Duration;

use chrono::{DateTime, Utc};

use super::accounting::TaskAccounting;
use super::ledger::PromptLedger;
use super::state::WatchState;
use super::tail::{baseline_offset_since, TranscriptTail};
use crate::acp::types::AcpEvent;

mod overlay;

fn async_launch(task_id: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu-launch",
                "content": "launched"
            }]
        },
        "toolUseResult": {
            "status": "async_launched",
            "agentId": task_id
        }
    })
}

fn shell_launch(task_id: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": [] },
        "toolUseResult": { "backgroundTaskId": task_id }
    })
}

fn task_output(task_id: &str, status: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": [] },
        "toolUseResult": {
            "task": { "task_id": task_id, "status": status }
        }
    })
}

fn assistant_tool(name: &str, input: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": "assistant",
        "message": {
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": "toolu-action",
                "name": name,
                "input": input
            }]
        }
    })
}

fn notification(task_id: &str) -> serde_json::Value {
    let text = format!(
        "<task-notification>\n<task-id>{task_id}</task-id>\n\
         <tool-use-id>toolu-launch</tool-use-id>\n<status>completed</status>\n\
         <summary>Agent finished</summary>\n<result>All checks passed</result>\n\
         </task-notification>"
    );
    serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": text }
    })
}

#[test]
fn task_output_and_explicit_stop_clear_outstanding_tasks() {
    let mut accounting = TaskAccounting::new();
    accounting.begin_tick(false, false);
    accounting.observe(&async_launch("agent-1"));
    accounting.observe(&shell_launch("shell-1"));
    assert_eq!(accounting.outstanding(), 2);

    accounting.observe(&task_output("shell-1", "running"));
    assert_eq!(accounting.outstanding(), 2);
    accounting.observe(&task_output("shell-1", "completed"));
    assert_eq!(accounting.outstanding(), 1);

    accounting.observe(&assistant_tool(
        "TaskStop",
        serde_json::json!({ "task_id": "agent-1" }),
    ));
    assert_eq!(accounting.outstanding(), 0);
}

#[test]
fn task_accounting_expires_abandoned_work_at_the_keepalive_limit() {
    let mut accounting = TaskAccounting::new();
    accounting.begin_tick(false, false);
    accounting.observe(&async_launch("agent-1"));
    assert_eq!(accounting.outstanding(), 1);

    assert!(accounting.expire(Duration::ZERO));
    assert_eq!(accounting.outstanding(), 0);
    assert!(!accounting.expire(Duration::ZERO));
}

#[test]
fn send_message_rearms_a_settled_agent_for_the_new_held_turn() {
    let mut accounting = TaskAccounting::new();
    accounting.begin_tick(true, false);
    accounting.observe(&async_launch("agent-1"));
    let settled = accounting.observe(&notification("agent-1"));
    assert_eq!(settled.len(), 1);
    assert!(settled[0].wire_visible);
    assert_eq!(accounting.outstanding(), 0);

    accounting.begin_tick(false, false);
    accounting.begin_tick(true, false);
    accounting.observe(&assistant_tool(
        "SendMessage",
        serde_json::json!({ "to": "agent-1", "message": "continue" }),
    ));
    assert_eq!(accounting.outstanding(), 1);

    let settled = accounting.observe(&notification("agent-1"));
    assert_eq!(settled.len(), 1);
    assert!(settled[0].wire_visible);
}

#[test]
fn abnormal_turn_end_releases_held_turn_suppression() {
    let mut accounting = TaskAccounting::new();
    accounting.begin_tick(true, false);
    accounting.observe(&async_launch("agent-1"));
    accounting.begin_tick(false, true);

    let settled = accounting.observe(&notification("agent-1"));
    assert_eq!(settled.len(), 1);
    assert!(!settled[0].wire_visible);
}

#[test]
fn fork_baseline_skips_new_metadata_and_copied_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fork.jsonl");
    let mut file = std::fs::File::create(&path).unwrap();
    let metadata = r#"{"type":"queue-operation","timestamp":"2026-07-16T10:00:01Z"}"#;
    let copied = r#"{"type":"user","timestamp":"2026-07-16T09:00:00Z","message":{"role":"user","content":"old"}}"#;
    let fresh = r#"{"type":"user","timestamp":"2026-07-16T10:00:02Z","message":{"role":"user","content":"new"}}"#;
    writeln!(file, "{metadata}").unwrap();
    writeln!(file, "{copied}").unwrap();
    writeln!(file, "{fresh}").unwrap();

    let epoch: DateTime<Utc> = "2026-07-16T10:00:00Z".parse().unwrap();
    let expected = metadata.len() as u64 + 1 + copied.len() as u64 + 1;
    assert_eq!(baseline_offset_since(&path, epoch.into()), Some(expected));
}

#[test]
fn prompt_ledger_consumes_each_sent_prompt_once() {
    let ledger = PromptLedger::new();
    ledger.record_text("repeat this");

    assert!(ledger.consume_matching("repeat this\n<system-reminder>extra</system-reminder>"));
    assert!(!ledger.consume_matching("repeat this"));
}

#[test]
fn transcript_tail_waits_for_a_complete_jsonl_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tail.jsonl");
    std::fs::write(&path, br#"{"type":"user""#).unwrap();
    let mut tail = TranscriptTail::from_offset(0);

    assert!(tail.read_new_lines(&path).unwrap().is_empty());
    assert_eq!(tail.committed(), 0);

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(file, "}}").unwrap();
    let lines = tail.read_new_lines(&path).unwrap();
    assert_eq!(lines, vec![r#"{"type":"user"}"#]);
    assert_eq!(tail.committed(), std::fs::metadata(&path).unwrap().len());
}

fn append_records(path: &std::path::Path, records: &[serde_json::Value]) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    for record in records {
        serde_json::to_writer(&mut file, record).unwrap();
        writeln!(file).unwrap();
    }
}

fn user_record(id: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "user",
        "uuid": id,
        "timestamp": "2026-07-16T10:00:00Z",
        "message": { "role": "user", "content": text }
    })
}

fn assistant_record(id: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "assistant",
        "uuid": id,
        "timestamp": "2026-07-16T10:00:01Z",
        "message": {
            "role": "assistant",
            "model": "claude-sonnet-4-6",
            "content": [{ "type": "text", "text": text }]
        }
    })
}

fn unpack_background(event: AcpEvent) -> (usize, u32, usize, u64) {
    match event {
        AcpEvent::BackgroundActivity {
            turns,
            outstanding,
            settled,
            watermark,
            ..
        } => (turns.len(), outstanding, settled.len(), watermark),
        other => panic!("expected background activity, got {other:?}"),
    }
}

#[test]
fn prompt_ledger_excludes_foreground_but_not_a_later_same_text_cron_turn() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    std::fs::File::create(&path).unwrap();
    let ledger = PromptLedger::new();
    ledger.record_text("repeat this");
    let mut watcher = WatchState::with_file_for_test("session-1", path.clone());

    append_records(
        &path,
        &[
            user_record("user-1", "repeat this"),
            assistant_record("assistant-1", "foreground"),
        ],
    );
    assert!(watcher
        .tick(&ledger, "D:/work", "connection-1", true, false)
        .is_none());

    append_records(
        &path,
        &[
            user_record("user-2", "repeat this"),
            assistant_record("assistant-2", "cron reply"),
        ],
    );
    let event = watcher
        .tick(&ledger, "D:/work", "connection-1", false, false)
        .expect("same-text refire is out of turn");
    let (turns, outstanding, settled, watermark) = unpack_background(event);
    assert_eq!((turns, outstanding, settled), (2, 0, 0));
    assert_eq!(watermark, std::fs::metadata(&path).unwrap().len());
}
