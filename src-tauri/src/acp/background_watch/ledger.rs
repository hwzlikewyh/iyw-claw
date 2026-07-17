use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::acp::types::PromptInputBlock;

const LEDGER_TTL: Duration = Duration::from_secs(600);
const LEDGER_CAP: usize = 32;

struct LedgerEntry {
    fingerprint: String,
    recorded_at: Instant,
}

pub(crate) struct PromptLedger {
    entries: Mutex<VecDeque<LedgerEntry>>,
}

impl PromptLedger {
    pub(super) fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
        }
    }

    pub(crate) fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    pub(crate) fn record_prompt_blocks(&self, blocks: &[PromptInputBlock]) {
        let fingerprint = blocks.iter().find_map(|block| match block {
            PromptInputBlock::Text { text } => {
                let text = text.trim();
                (!text.is_empty()).then(|| text.to_string())
            }
            _ => None,
        });
        let Some(fingerprint) = fingerprint else {
            return;
        };
        let mut entries = self.entries.lock().unwrap_or_else(|lock| lock.into_inner());
        entries.push_back(LedgerEntry {
            fingerprint,
            recorded_at: Instant::now(),
        });
        while entries.len() > LEDGER_CAP {
            entries.pop_front();
        }
    }

    pub(super) fn consume_matching(&self, initiator_text: &str) -> bool {
        let text = initiator_text.trim();
        if text.is_empty() {
            return false;
        }
        let mut entries = self.entries.lock().unwrap_or_else(|lock| lock.into_inner());
        entries.retain(|entry| entry.recorded_at.elapsed() < LEDGER_TTL);
        let Some(position) = entries.iter().position(|entry| {
            text == entry.fingerprint || text.starts_with(entry.fingerprint.as_str())
        }) else {
            return false;
        };
        entries.remove(position);
        true
    }

    #[cfg(test)]
    pub(super) fn record_text(&self, text: &str) {
        self.record_prompt_blocks(&[PromptInputBlock::Text {
            text: text.to_string(),
        }]);
    }
}
