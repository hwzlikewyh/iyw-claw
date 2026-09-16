use super::{nonempty_str, BackgroundSettledInfo, TaskAccounting};

impl TaskAccounting {
    pub(in crate::acp::background_watch) fn restore(
        &mut self,
        path: &std::path::Path,
        offset: u64,
    ) -> bool {
        use std::io::{BufRead, Read};
        let Ok(file) = std::fs::File::open(path) else {
            return false;
        };
        let prompting = self.currently_prompting;
        self.currently_prompting = false;
        for line in std::io::BufReader::new(file.take(offset))
            .lines()
            .map_while(Result::ok)
        {
            if let Ok(value) = serde_json::from_str(&line) {
                let _ = self.observe(&value);
            }
        }
        self.currently_prompting = prompting;
        self.uncertain_ids.extend(self.tasks.keys().cloned());
        tracing::debug!(
            outstanding = self.tasks.len(),
            "[bg-watch] restored unresolved tasks as unconfirmed"
        );
        !self.tasks.is_empty()
    }
    pub(super) fn remember_launch_tool(
        &mut self,
        value: &serde_json::Value,
        result: &serde_json::Value,
    ) {
        let id = nonempty_str(result.get("agentId"))
            .or_else(|| nonempty_str(result.get("backgroundTaskId")));
        let call = value
            .pointer("/message/content")
            .and_then(|v| v.as_array())
            .and_then(|blocks| blocks.iter().find(|b| b["type"] == "tool_result"))
            .and_then(|block| nonempty_str(block.get("tool_use_id")));
        if let (Some(id), Some(call)) = (id, call) {
            self.launch_tools.insert(id.to_string(), call.to_string());
        }
    }

    pub(super) fn observe_tool_results(
        &mut self,
        value: &serde_json::Value,
    ) -> Vec<BackgroundSettledInfo> {
        let Some(blocks) = value.pointer("/message/content").and_then(|v| v.as_array()) else {
            return Vec::new();
        };
        let mut settled = Vec::new();
        for block in blocks.iter().filter(|b| b["type"] == "tool_result") {
            let Some(call) = nonempty_str(block.get("tool_use_id")) else {
                continue;
            };
            if let Some(id) = self.pending_resumes.remove(call) {
                if block["is_error"] != true {
                    self.observe_resume(&id);
                }
            }
            let Some(id) = self.pending_stops.remove(call) else {
                continue;
            };
            if block["is_error"] == true {
                tracing::warn!(
                    task_id = id,
                    tool_use_id = call,
                    "[bg-watch] stop failed; task remains unresolved"
                );
                continue;
            }
            settled.push(self.settle(&id, "stopped"));
        }
        settled
    }
}
