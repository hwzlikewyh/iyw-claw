use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{AgentType, MessageTurn};

pub const TRANSCRIPT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptHeader {
    pub version: u32,
    pub agent: AgentType,
    pub session_id: String,
    pub cwd: String,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continues_from: Option<String>,
}

impl TranscriptHeader {
    pub fn new(agent: AgentType, session_id: &str, cwd: &str) -> Self {
        Self {
            version: TRANSCRIPT_SCHEMA_VERSION,
            agent,
            session_id: session_id.to_string(),
            cwd: cwd.to_string(),
            started_at: Utc::now(),
            continues_from: None,
        }
    }

    pub fn continuing_from(mut self, session_id: &str) -> Self {
        self.continues_from = Some(session_id.to_string());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptRecord {
    Header {
        header: TranscriptHeader,
    },
    Turn {
        version: u32,
        observed_at: DateTime<Utc>,
        turn: MessageTurn,
    },
}

impl TranscriptRecord {
    pub fn header(header: TranscriptHeader) -> Self {
        Self::Header { header }
    }

    pub fn turn(turn: MessageTurn) -> Self {
        Self::Turn {
            version: TRANSCRIPT_SCHEMA_VERSION,
            observed_at: Utc::now(),
            turn,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TranscriptData {
    pub header: Option<TranscriptHeader>,
    pub turns: Vec<MessageTurn>,
    pub invalid_lines: usize,
}

impl TranscriptData {
    pub(super) fn apply(&mut self, record: TranscriptRecord, index: &mut HashMap<String, usize>) {
        match record {
            TranscriptRecord::Header { header }
                if header.version == TRANSCRIPT_SCHEMA_VERSION && self.header.is_none() =>
            {
                self.header = Some(header);
            }
            TranscriptRecord::Turn { version, turn, .. }
                if version == TRANSCRIPT_SCHEMA_VERSION =>
            {
                if let Some(position) = index.get(&turn.id).copied() {
                    self.turns[position] = turn;
                } else {
                    index.insert(turn.id.clone(), self.turns.len());
                    self.turns.push(turn);
                }
            }
            _ => self.invalid_lines += 1,
        }
    }

    pub(super) fn merge(&mut self, next: TranscriptData) {
        let mut index = self
            .turns
            .iter()
            .enumerate()
            .map(|(position, turn)| (turn.id.clone(), position))
            .collect::<HashMap<_, _>>();
        for turn in next.turns {
            if let Some(position) = index.get(&turn.id).copied() {
                self.turns[position] = turn;
            } else {
                index.insert(turn.id.clone(), self.turns.len());
                self.turns.push(turn);
            }
        }
        if self.header.is_none() {
            self.header = next.header;
        }
        self.invalid_lines += next.invalid_lines;
    }
}
