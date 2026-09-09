use std::collections::HashMap;
use std::sync::Mutex;

use crate::session::turn_context::TurnContext;

const MAX_DESCRIPTION_CHARS: usize = 120;

#[derive(Default)]
struct CommandDescriptions(Mutex<HashMap<String, String>>);

impl TurnContext {
    pub(crate) fn record_command_description(&self, call_id: &str, description: Option<&str>) {
        let Some(description) = description else {
            return;
        };
        let description: String = description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .filter(|character| !character.is_control())
            .take(MAX_DESCRIPTION_CHARS)
            .collect();
        if description.is_empty() {
            return;
        }
        // 使用回合已有的扩展状态，异步完成事件仍按原始调用 ID 读取。
        let descriptions = self
            .extension_data
            .get_or_init(CommandDescriptions::default);
        descriptions
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(call_id.to_string(), description);
    }

    pub(crate) fn command_description(&self, call_id: &str) -> Option<String> {
        let descriptions = self.extension_data.get::<CommandDescriptions>()?;
        let result = descriptions
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(call_id)
            .cloned();
        result
    }
}
