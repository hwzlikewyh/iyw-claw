use std::io;

use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};

use crate::web::event_bridge::{emit_event, EventEmitter};

const ERROR_LIMIT: usize = 2_000;
const ERROR_BUFFER_BYTES: usize = 4096;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    component_id: String,
    phase: String,
    #[serde(default)]
    percent: Option<u32>,
    #[serde(default)]
    downloaded: u64,
    #[serde(default)]
    total: u64,
    #[serde(default)]
    message: String,
}

pub(super) struct ProgressSink<'a> {
    pub task_id: &'a str,
    pub emitter: &'a EventEmitter,
    pub on_progress: &'a (dyn Fn(&str) + Send + Sync),
    pub components: Option<&'a str>,
}

pub(super) async fn forward_progress(
    stream: impl AsyncRead + Unpin,
    sink: &ProgressSink<'_>,
) -> io::Result<()> {
    let mut lines = BufReader::new(stream).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(event) = serde_json::from_str::<Progress>(&line) else {
            continue;
        };
        if sink.components.is_some_and(|ids| {
            event.component_id != "environment"
                && !ids.split(',').any(|id| id == event.component_id)
        }) {
            continue;
        }
        forward_event(event, sink);
    }
    Ok(())
}

fn forward_event(event: Progress, sink: &ProgressSink<'_>) {
    if event.phase == "optional-skipped" {
        tracing::warn!(
            task_id = sink.task_id,
            component_id = %event.component_id,
            "optional environment component skipped during repair"
        );
    }
    let percent = event
        .percent
        .map(|value| format!(" ({value}%)"))
        .unwrap_or_default();
    (sink.on_progress)(&format!("{}: {}{percent}", event.component_id, event.phase));
    if matches!(
        event.phase.as_str(),
        "reused" | "reuse-checking" | "reuse-checked"
    ) {
        return;
    }
    let component = (event.component_id != "environment").then_some(event.component_id);
    emit_event(
        sink.emitter,
        "app://bootstrap-init",
        serde_json::json!({
            "taskId": sink.task_id, "phase": bootstrap_phase(&event.phase), "component": component,
            "percent": event.percent,
            "downloaded": event.downloaded, "total": event.total, "message": event.message,
        }),
    );
}

fn bootstrap_phase(phase: &str) -> &str {
    match phase {
        "downloaded" | "cached" | "extracting" | "component-prepared" | "optional-skipped"
        | "prepared" => "staging",
        "verified" => "health_check",
        "committed" => "ready",
        phase => phase,
    }
}

pub(super) async fn error_tail(mut stream: impl AsyncRead + Unpin) -> io::Result<String> {
    let mut result = Vec::new();
    let mut buffer = [0_u8; ERROR_BUFFER_BYTES];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        result.extend_from_slice(&buffer[..count]);
        if result.len() > ERROR_LIMIT {
            result.drain(..result.len() - ERROR_LIMIT);
        }
    }
    Ok(String::from_utf8_lossy(&result).trim().to_string())
}
