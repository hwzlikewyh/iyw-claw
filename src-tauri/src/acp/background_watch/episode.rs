use std::hash::{Hash, Hasher};

use crate::models::message::MessageTurn;
use crate::parsers::claude::{is_meta_message, slash_command_display, CONTEXT_CONTINUATION_PREFIX};
use crate::parsers::claude_background::TaskNotification;
use crate::parsers::claude_tail::ClaudeTailAccumulator;

use super::ledger::PromptLedger;
use super::state::{Episode, Mode, WatchState};

const MAX_EPISODE_MESSAGES: usize = 512;
const FORCE_ROTATE_MESSAGES: usize = MAX_EPISODE_MESSAGES * 2;

impl WatchState {
    pub(super) fn classify_and_feed(
        &mut self,
        mut value: serde_json::Value,
        ledger: &PromptLedger,
        cwd: &str,
        changed_turns: &mut Vec<MessageTurn>,
    ) {
        if let Some(initiator) = turn_initiator_text(&value) {
            if ledger.consume_matching(&initiator) {
                self.collect_changed_turns(cwd, changed_turns);
                self.episode = None;
                self.mode = Mode::Foreground;
                return;
            }
            self.start_episode_if_needed(&initiator, cwd, changed_turns);
            self.mode = Mode::Background;
        }
        if matches!(self.mode, Mode::Background) {
            self.rotate_oversized_episode(cwd, changed_turns);
            if let Some(episode) = self.episode.as_mut() {
                strip_private_user_context_from_record(&mut value);
                episode.accumulator.feed_value(value);
            }
        }
    }

    fn start_episode_if_needed(
        &mut self,
        initiator: &str,
        cwd: &str,
        changed_turns: &mut Vec<MessageTurn>,
    ) {
        let rotate = self
            .episode
            .as_ref()
            .is_some_and(|episode| episode.accumulator.message_count() >= MAX_EPISODE_MESSAGES);
        if matches!(self.mode, Mode::Background) && self.episode.is_some() && !rotate {
            return;
        }
        if rotate {
            self.collect_changed_turns(cwd, changed_turns);
        }
        self.episode = Some(self.new_episode(notification_task_id(initiator)));
    }

    fn rotate_oversized_episode(&mut self, cwd: &str, changed_turns: &mut Vec<MessageTurn>) {
        let rotate = self
            .episode
            .as_ref()
            .is_some_and(|episode| episode.accumulator.message_count() >= FORCE_ROTATE_MESSAGES);
        if !rotate {
            return;
        }
        let origin = self
            .episode
            .as_ref()
            .and_then(|episode| episode.origin_task_id.clone());
        self.collect_changed_turns(cwd, changed_turns);
        self.episode = Some(self.new_episode(origin));
    }

    fn new_episode(&mut self, origin_task_id: Option<String>) -> Episode {
        let base = self.tail.committed().max(self.last_episode_base + 1);
        self.last_episode_base = base;
        Episode {
            start_offset: base,
            accumulator: ClaudeTailAccumulator::new(self.file.clone()),
            emitted_hashes: Default::default(),
            origin_task_id,
        }
    }

    pub(super) fn collect_changed_turns(&mut self, cwd: &str, out: &mut Vec<MessageTurn>) {
        let Some(episode) = self.episode.as_mut() else {
            return;
        };
        let turns = episode.accumulator.collect_turns(Some(cwd));
        for (index, mut turn) in turns.into_iter().enumerate() {
            turn.id = format!("bg-{}-{index}", episode.start_offset);
            let hash = hash_turn(&turn);
            if episode.emitted_hashes.get(&turn.id) == Some(&hash) {
                continue;
            }
            episode.emitted_hashes.insert(turn.id.clone(), hash);
            self.turn_origins
                .insert(turn.id.clone(), episode.origin_task_id.clone());
            out.push(turn);
        }
    }
}

fn turn_initiator_text(value: &serde_json::Value) -> Option<String> {
    if value.get("type").and_then(|kind| kind.as_str()) != Some("user") {
        return None;
    }
    let content = value.get("message")?.get("content")?;
    if let Some(text) = content.as_str() {
        let text = crate::user_memory::strip_user_context(text);
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        if text.starts_with(CONTEXT_CONTINUATION_PREFIX) {
            return None;
        }
        return Some(slash_command_display(text).unwrap_or_else(|| text.to_string()));
    }
    let blocks = content.as_array()?;
    if !blocks.is_empty()
        && blocks
            .iter()
            .all(|block| block.get("type").and_then(|kind| kind.as_str()) == Some("tool_result"))
    {
        return None;
    }
    if is_meta_message(value) {
        return None;
    }
    let text = crate::user_memory::strip_user_context(&user_text(blocks)?);
    let text = text.trim();
    (!text.is_empty() && !text.starts_with(CONTEXT_CONTINUATION_PREFIX)).then(|| text.to_string())
}

fn user_text(blocks: &[serde_json::Value]) -> Option<String> {
    let texts: Vec<&str> = blocks
        .iter()
        .filter(|block| block.get("type").and_then(|kind| kind.as_str()) == Some("text"))
        .filter_map(|block| block.get("text").and_then(|text| text.as_str()))
        .collect();
    (!texts.is_empty()).then(|| texts.join("\n"))
}

fn notification_task_id(text: &str) -> Option<String> {
    TaskNotification::parse(text.trim_start()).map(|notification| notification.task_id)
}

fn strip_private_user_context_from_record(value: &mut serde_json::Value) {
    let Some(content) = value
        .get_mut("message")
        .and_then(|message| message.get_mut("content"))
    else {
        return;
    };
    match content {
        serde_json::Value::String(text) => {
            *text = crate::user_memory::strip_user_context(text);
        }
        serde_json::Value::Array(blocks) => {
            for block in blocks.iter_mut() {
                if block.get("type").and_then(|kind| kind.as_str()) != Some("text") {
                    continue;
                }
                let Some(text) = block.get_mut("text").and_then(|text| text.as_str()) else {
                    continue;
                };
                let visible = crate::user_memory::strip_user_context(text);
                block["text"] = serde_json::Value::String(visible);
            }
        }
        _ => {}
    }
}

fn hash_turn(turn: &MessageTurn) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(turn)
        .unwrap_or_else(|_| turn.blocks.len().to_string())
        .hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_memory::{USER_CONTEXT_END, USER_CONTEXT_START};

    fn context_prefixed_user_record(prompt: &str) -> serde_json::Value {
        serde_json::json!({
            "type": "user",
            "message": {
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "{USER_CONTEXT_START}\nPrivate profile\n{USER_CONTEXT_END}"
                        )
                    },
                    { "type": "text", "text": prompt }
                ]
            }
        })
    }

    #[test]
    fn context_prefixed_foreground_prompt_matches_ledger_without_background_event() {
        let mut watcher = WatchState::with_file_for_test(
            "session",
            std::path::PathBuf::from("unused-transcript.jsonl"),
        );
        let ledger = PromptLedger::new();
        ledger.record_text("actual prompt");
        let mut turns = Vec::new();

        watcher.classify_and_feed(
            context_prefixed_user_record("actual prompt"),
            &ledger,
            "",
            &mut turns,
        );
        watcher.collect_changed_turns("", &mut turns);

        assert!(
            turns.is_empty(),
            "foreground prompt must not become background work"
        );
    }

    #[test]
    fn background_activity_payload_strips_private_context_envelope() {
        let mut watcher = WatchState::with_file_for_test(
            "session",
            std::path::PathBuf::from("unused-transcript.jsonl"),
        );
        let ledger = PromptLedger::new();
        let mut turns = Vec::new();

        watcher.classify_and_feed(
            context_prefixed_user_record("background prompt"),
            &ledger,
            "",
            &mut turns,
        );
        watcher.collect_changed_turns("", &mut turns);
        let payload = serde_json::to_string(&turns).unwrap();
        assert!(!payload.contains(USER_CONTEXT_START));
        assert!(!payload.contains("Private profile"));
        assert!(payload.contains("background prompt"));
    }
}
