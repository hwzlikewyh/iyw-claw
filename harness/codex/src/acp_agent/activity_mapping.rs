use serde_json::{json, Value};

use super::acp_mapping::Update;

pub(super) fn observation(method: &str, params: &Value) -> Option<Update> {
    let activity = match method {
        "item/commandExecution/terminalInteraction" => {
            if params.get("stdin")?.as_str()? != "" {
                return None;
            }
            json!({
                "kind": "terminal_poll",
                "item_id": params.get("itemId")?.as_str()?,
                "process_id": params.get("processId")?.as_str()?,
            })
        }
        "error" if params.get("willRetry")?.as_bool()? => {
            let error = params.get("error");
            let detail = error
                .and_then(|error| {
                    error
                        .get("additionalDetails")
                        .and_then(Value::as_str)
                        .or_else(|| error.get("message").and_then(Value::as_str))
                })
                .map(crate::diagnostics::safe_detail);
            json!({ "kind": "retry", "detail": detail })
        }
        _ => return None,
    };
    Some(Update {
        method: "session_info_update",
        params: json!({ "_meta": { "iyw": { "activity": activity } } }),
    })
}
