use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::models::MessageTurn;

use super::update_state::UpdateAccumulator;

#[derive(Default)]
pub(super) struct ParsedUpdates {
    pub(super) turns: Vec<MessageTurn>,
    pub(super) first_ts: Option<DateTime<Utc>>,
    pub(super) last_ts: Option<DateTime<Utc>>,
    pub(super) content_events: u32,
    pub(super) first_user_text: Option<String>,
    pub(super) model: Option<String>,
}

pub(super) fn parse_updates(path: &Path) -> ParsedUpdates {
    let Ok(file) = fs::File::open(path) else {
        return ParsedUpdates::default();
    };
    let mut accumulator = UpdateAccumulator::default();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str(&line) else {
            continue;
        };
        accumulator.consume(&value);
    }
    accumulator.finish()
}
