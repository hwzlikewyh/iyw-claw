use std::collections::HashMap;

use serde_json::Value;

pub(super) fn update_protocol_maps(
    method: &str,
    params: &Value,
    session_id: Option<&str>,
    sessions: &mut HashMap<String, String>,
    frames: &mut HashMap<String, String>,
) {
    if method == "Target.attachedToTarget" {
        attach_session(params, sessions);
    } else if method == "Target.detachedFromTarget" {
        if let Some(session) = params.get("sessionId").and_then(Value::as_str) {
            if let Some(target) = sessions.remove(session) {
                frames.retain(|_, mapped| mapped != &target);
            }
        }
    } else if method == "Page.frameNavigated" {
        map_frame(params, session_id, sessions, frames);
    } else if method == "Page.frameDetached" {
        if let Some(frame) = params.get("frameId").and_then(Value::as_str) {
            frames.remove(frame);
        }
    }
}

fn attach_session(params: &Value, sessions: &mut HashMap<String, String>) {
    let session = params.get("sessionId").and_then(Value::as_str);
    let target = params
        .pointer("/targetInfo/targetId")
        .and_then(Value::as_str);
    if let (Some(session), Some(target)) = (session, target) {
        sessions.insert(session.to_string(), target.to_string());
    }
}

fn map_frame(
    params: &Value,
    session_id: Option<&str>,
    sessions: &HashMap<String, String>,
    frames: &mut HashMap<String, String>,
) {
    let frame = params.pointer("/frame/id").and_then(Value::as_str);
    let target = session_id.and_then(|id| sessions.get(id));
    if let (Some(frame), Some(target)) = (frame, target) {
        frames.insert(frame.to_string(), target.clone());
    }
}
