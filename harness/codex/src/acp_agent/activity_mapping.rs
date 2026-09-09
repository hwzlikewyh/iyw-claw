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
            json!({ "kind": "retry" })
        }
        _ => return None,
    };
    Some(Update {
        method: "session_info_update",
        params: json!({ "_meta": { "iyw": { "activity": activity } } }),
    })
}
