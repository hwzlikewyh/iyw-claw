use serde_json::Value;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct BackgroundCommands {
    owners: HashMap<String, (String, String)>,
}

impl BackgroundCommands {
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
