use std::collections::HashMap;
use std::path::PathBuf;

use crate::acp::types::{AcpEvent, BackgroundSettledInfo};
use crate::models::message::{ContentBlock, MessageTurn, TurnRole};
use crate::parsers::claude_tail::ClaudeTailAccumulator;

use super::accounting::TaskAccounting;
use super::ledger::PromptLedger;
use super::tail::TranscriptTail;

pub(super) enum Mode {
    Foreground,
    Background,
}

pub(super) struct Episode {
    pub(super) start_offset: u64,
    pub(super) accumulator: ClaudeTailAccumulator,
    pub(super) emitted_hashes: HashMap<String, u64>,
    pub(super) origin_task_id: Option<String>,
}

pub(super) struct WatchState {
    pub(super) session_id: String,
    pub(super) file: PathBuf,
    pub(super) tail: TranscriptTail,
    pub(super) last_stat: Option<(Option<std::time::SystemTime>, u64)>,
    pub(super) mode: Mode,
    pub(super) episode: Option<Episode>,
    pub(super) accounting: TaskAccounting,
    pub(super) turn_origins: HashMap<String, Option<String>>,
    pub(super) last_emitted_outstanding: u32,
    pub(super) last_episode_base: u64,
    last_disk_activity: Option<std::time::Instant>,
}

impl WatchState {
    #[cfg(test)]
    pub(super) fn with_file_for_test(session_id: &str, file: PathBuf) -> Self {
        Self::new(session_id.to_string(), file, 0)
    }

    pub(super) fn new(session_id: String, file: PathBuf, offset: u64) -> Self {
        Self {
            session_id,
            file,
            tail: TranscriptTail::from_offset(offset),
            last_stat: None,
            mode: Mode::Foreground,
            episode: None,
            accounting: TaskAccounting::new(),
            turn_origins: HashMap::new(),
            last_emitted_outstanding: 0,
            last_episode_base: 0,
            last_disk_activity: None,
        }
    }

    pub(super) fn rearm(&mut self, session_id: String, file: PathBuf, offset: u64) {
        self.session_id = session_id;
        self.file = file;
        self.tail = TranscriptTail::from_offset(offset);
        self.last_stat = None;
        self.mode = Mode::Foreground;
        self.episode = None;
        self.turn_origins.clear();
        self.last_emitted_outstanding = 0;
        self.last_episode_base = 0;
        self.last_disk_activity = None;
    }

    pub(super) fn poll_delay(&self) -> std::time::Duration {
        let recently_active = self
            .last_disk_activity
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(30));
        if self.accounting.outstanding() > 0 || recently_active {
            std::time::Duration::from_secs(1)
        } else {
            std::time::Duration::from_secs(3)
        }
    }

    pub(super) fn tick(
        &mut self,
        ledger: &PromptLedger,
        cwd: &str,
        _connection_id: &str,
        prompting: bool,
        ended_abnormally: bool,
    ) -> Option<AcpEvent> {
        self.accounting.begin_tick(prompting, ended_abnormally);
        let max_age = crate::acp::session_state::background_keepalive_max_age()
            .to_std()
            .unwrap_or(std::time::Duration::from_secs(3600));
        let expired = self.accounting.expire(max_age);
        let lines = match self.read_changed_lines() {
            Some(lines) => lines,
            None if expired => Vec::new(),
            None => return None,
        };
        let mut turns = Vec::new();
        let mut settled = Vec::new();
        self.process_lines(lines, ledger, cwd, &mut turns, &mut settled);
        self.collect_changed_turns(cwd, &mut turns);
        self.suppress_held_turns(&mut turns);
        self.build_event(turns, settled)
    }

    fn read_changed_lines(&mut self) -> Option<Vec<String>> {
        let metadata = std::fs::metadata(&self.file).ok()?;
        let stat = (metadata.modified().ok(), metadata.len());
        if self.last_stat.as_ref() == Some(&stat) {
            return None;
        }
        self.last_stat = Some(stat);
        if metadata.len() < self.tail.committed() {
            self.tail.reset(metadata.len());
            self.episode = None;
            self.mode = Mode::Foreground;
            return Some(Vec::new());
        }
        self.tail.read_new_lines(&self.file).ok()
    }

    fn process_lines(
        &mut self,
        lines: Vec<String>,
        ledger: &PromptLedger,
        cwd: &str,
        turns: &mut Vec<MessageTurn>,
        settled: &mut Vec<BackgroundSettledInfo>,
    ) {
        if !lines.is_empty() {
            self.last_disk_activity = Some(std::time::Instant::now());
        }
        for line in lines {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            settled.extend(self.accounting.observe(&value));
            self.classify_and_feed(value, ledger, cwd, turns);
        }
    }

    fn suppress_held_turns(&mut self, turns: &mut Vec<MessageTurn>) {
        turns.retain(|turn| {
            let origin = self.turn_origins.remove(&turn.id).flatten();
            !origin.is_some_and(|task_id| self.accounting.is_held_task(&task_id))
        });
    }

    fn build_event(
        &mut self,
        mut turns: Vec<MessageTurn>,
        settled: Vec<BackgroundSettledInfo>,
    ) -> Option<AcpEvent> {
        strip_private_user_context_from_turns(&mut turns);
        let outstanding = self.accounting.outstanding();
        let accounting_changed = outstanding != self.last_emitted_outstanding;
        if turns.is_empty() && settled.is_empty() && !accounting_changed {
            return None;
        }
        self.last_emitted_outstanding = outstanding;
        Some(AcpEvent::BackgroundActivity {
            session_id: self.session_id.clone(),
            turns,
            outstanding,
            settled,
            watermark: self.tail.committed(),
        })
    }
}

fn strip_private_user_context_from_turns(turns: &mut Vec<MessageTurn>) {
    for turn in turns.iter_mut() {
        if !matches!(turn.role, TurnRole::User) {
            continue;
        }
        turn.blocks.retain_mut(|block| match block {
            ContentBlock::Text { text } => {
                *text = crate::user_memory::strip_user_context(text);
                !text.trim().is_empty()
            }
            _ => true,
        });
    }
    turns.retain(|turn| !matches!(turn.role, TurnRole::User) || !turn.blocks.is_empty());
}
