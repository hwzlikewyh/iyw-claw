use serde::Serialize;

use crate::web::event_bridge::EventEmitter;

const RUNTIME_BOOTSTRAP_EVENT: &str = "app://runtime-bootstrap";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeComponentStatus {
    Ready,
    Installed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeComponentReport {
    pub status: RuntimeComponentStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeBootstrapReport {
    pub node: RuntimeComponentReport,
    pub git: RuntimeComponentReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RuntimeBootstrapEventKind {
    Started,
    Log,
    Progress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeBootstrapEvent {
    task_id: String,
    kind: RuntimeBootstrapEventKind,
    component: Option<String>,
    percent: Option<u8>,
    payload: String,
}

pub(super) fn emit(
    emitter: &EventEmitter,
    task_id: &str,
    kind: RuntimeBootstrapEventKind,
    component: Option<String>,
    percent: Option<u8>,
    payload: impl Into<String>,
) {
    crate::web::event_bridge::emit_event(
        emitter,
        RUNTIME_BOOTSTRAP_EVENT,
        RuntimeBootstrapEvent {
            task_id: task_id.to_string(),
            kind,
            component,
            percent,
            payload: payload.into(),
        },
    );
}
