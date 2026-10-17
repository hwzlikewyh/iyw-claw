use serde_json::{json, Value};
use std::collections::HashMap;

const OUTPUT_PREVIEW_CHARS: usize = 8_000;

#[derive(Default)]
pub(super) struct BackgroundCommands {
    owners: HashMap<String, (String, String)>,
    terminals: HashMap<String, Value>,
}

impl BackgroundCommands {
    pub(super) fn seed(&mut self, root: &str, tasks: &[Value]) {
        for task in tasks {
            let Some(id) = task["itemId"].as_str() else {
                continue;
            };
            self.owners
                .entry(id.into())
                .or_insert_with(|| (root.into(), String::new()));
            self.terminals
                .entry(id.into())
                .or_insert_with(|| task.clone());
        }
    }

    pub(super) fn native_update(
        &mut self,
        method: &str,
        params: &Value,
    ) -> Option<super::acp_mapping::Update> {
        self.remember_terminal(method, params);
        let id = params["itemId"]
            .as_str()
            .or_else(|| params["item"]["id"].as_str())?;
        let (thread, turn) = self.owners.get_mut(id)?;
        if params["threadId"].as_str() != Some(thread.as_str())
            || !turn.is_empty() && params["turnId"].as_str() != Some(turn.as_str())
        {
            return None;
        }
        if turn.is_empty() {
            *turn = params["turnId"].as_str()?.into();
        }
        let mut task = self.terminals.get(id)?.clone();
        match method {
            "item/started" => {}
            "item/commandExecution/outputDelta" => task["delta"] = preview(params.get("delta")?),
            "item/completed" => {
                task["status"] = json!(if params["item"]["status"] == "completed" {
                    "completed"
                } else {
                    "failed"
                });
                task["output"] = preview(&params["item"]["aggregatedOutput"]);
                self.terminals.remove(id);
            }
            _ => return None,
        }
        Some(super::acp_mapping::Update {
            method: "session_info_update",
            params: json!({"_meta":{"iyw":{"backgroundTask":task}}}),
        })
    }

    fn remember_terminal(&mut self, method: &str, params: &Value) {
        if method != "item/started" || params["item"]["type"] != "commandExecution" {
            return;
        }
        self.observe(method, params);
        let item = &params["item"];
        if let Some(id) = item["id"]
            .as_str()
            .filter(|_| item["processId"].is_string())
        {
            self.terminals.insert(id.into(), json!({
                "itemId": item["id"], "processId": item["processId"],
                "command": item["command"], "description": item["description"], "status":"running",
            }));
        }
    }

    pub(super) fn observe(&mut self, method: &str, params: &Value) {
        let Some(item) = params
            .get("item")
            .filter(|item| item["type"] == "commandExecution")
        else {
            return;
        };
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            return;
        };
        if method == "item/completed" {
            self.owners.remove(id);
            return;
        }
        if method != "item/started" {
            return;
        }
        let (Some(thread), Some(turn)) = (params["threadId"].as_str(), params["turnId"].as_str())
        else {
            return;
        };
        self.owners
            .insert(id.to_string(), (thread.to_string(), turn.to_string()));
    }

    /// 只接收登记过的命令在原线程、原轮次上的退出，不能借此恢复旧轮次。
    pub(super) fn take_completion(&mut self, method: &str, params: &Value) -> bool {
        if method != "item/completed" || params["item"]["type"] != "commandExecution" {
            return false;
        }
        let Some(id) = params["item"]["id"].as_str() else {
            return false;
        };
        let Some((thread, turn)) = self.owners.get(id) else {
            return false;
        };
        if params["threadId"].as_str() != Some(thread.as_str())
            || params["turnId"].as_str() != Some(turn.as_str())
        {
            return false;
        }
        if !matches!(
            params["item"]["status"].as_str(),
            Some("completed" | "failed" | "declined" | "cancelled" | "canceled")
        ) {
            return false;
        }
        self.owners.remove(id);
        true
    }
}

fn preview(value: &Value) -> Value {
    value
        .as_str()
        .map(|output| {
            let tail: Vec<_> = output.chars().rev().take(OUTPUT_PREVIEW_CHARS).collect();
            Value::String(tail.into_iter().rev().collect())
        })
        .unwrap_or(Value::Null)
}
