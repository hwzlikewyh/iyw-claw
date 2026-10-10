use super::super::decimal::Total;
use super::query::RequestRows;
use crate::models::{
    BackendConsumption, ConfirmedConsumption, DbConversationDetail, TurnRole, TurnUsage,
};
use crate::parsers::codex::billing::Receipt;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
struct Totals {
    total: Total,
    compact: Total,
    usage: [u64; 4],
}

impl Totals {
    fn add(&mut self, row: &super::query::Record, purpose: &str) -> bool {
        let Some(value) = row.actual_points.as_deref() else {
            return false;
        };
        if self.total.add(value).is_none() {
            return false;
        }
        for (total, count) in self.usage.iter_mut().zip([
            row.input_tokens,
            row.output_tokens,
            row.cache_read_tokens,
            row.cache_write_tokens,
        ]) {
            *total = total.saturating_add(count);
        }
        if purpose == "compaction" {
            let _ = self.compact.add(value);
        }
        true
    }

    fn usage(&self) -> TurnUsage {
        TurnUsage {
            estimated_points: None,
            input_tokens: self.usage[0],
            output_tokens: self.usage[1],
            cache_read_input_tokens: self.usage[2],
            cache_creation_input_tokens: self.usage[3],
        }
    }
}

fn consumption(
    requests: &[&Receipt],
    records: &HashMap<String, RequestRows>,
) -> ConfirmedConsumption {
    let mut totals = Totals::default();
    let mut seen = HashSet::new();
    let mut confirmed = false;
    let mut pending = false;
    let mut unavailable = requests.is_empty();
    for receipt in requests {
        let Some(rows) = records.get(&receipt.request_id) else {
            pending = true;
            continue;
        };
        pending |= rows.pending;
        for row in &rows.items {
            if !seen.insert((&row.request_id, &row.stage_key)) {
                continue;
            }
            if !totals.add(row, &receipt.purpose) {
                unavailable = true;
                continue;
            }
            confirmed = true;
            unavailable |=
                row.cost_status == "partial_stream" || row.cost_status == "usage_missing";
        }
    }
    let state = match (confirmed, pending, unavailable) {
        (true, false, false) => "confirmed",
        (true, _, _) => "partial",
        (false, true, _) => "pending",
        _ => "unavailable",
    };
    ConfirmedConsumption {
        amount: confirmed.then(|| totals.total.value()),
        state: state.into(),
        compaction_points: Some(totals.compact.value()),
        usage: confirmed.then(|| totals.usage()),
    }
}

pub(super) fn summarize(detail: &DbConversationDetail, source: Source<'_>) -> BackendConsumption {
    let Source {
        requests,
        messages,
        records,
    } = source;
    let mut result = BackendConsumption {
        session: consumption(&requests.iter().collect::<Vec<_>>(), records),
        ..Default::default()
    };
    let (last_turns, mut unverified) = last_display_turns(detail, messages);
    for (native, display) in last_turns {
        let correlated: Vec<_> = requests
            .iter()
            .filter(|receipt| {
                receipt.purpose == "generation"
                    && format!("{}:{}", receipt.thread_id, receipt.turn_id) == native
            })
            .collect();
        if correlated.is_empty() {
            unverified = true;
        }
        result
            .turns
            .insert(display, consumption(&correlated, records));
    }
    if unverified && result.session.state == "confirmed" {
        result.session.state = "partial".into();
    }
    result
}

pub(super) struct Source<'a> {
    pub(super) requests: &'a [Receipt],
    pub(super) messages: &'a HashMap<String, String>,
    pub(super) records: &'a HashMap<String, RequestRows>,
}

fn last_display_turns(
    detail: &DbConversationDetail,
    messages: &HashMap<String, String>,
) -> (HashMap<String, String>, bool) {
    let mut last_turns = HashMap::new();
    let mut unverified = false;
    for turn in detail
        .turns
        .iter()
        .filter(|turn| matches!(turn.role, TurnRole::Assistant))
    {
        let Some(native_turn) = turn
            .fork_message_id
            .as_ref()
            .and_then(|id| messages.get(id))
        else {
            unverified |= turn.usage.is_some() || turn.blocks.iter().any(|block| {
                matches!(block, crate::models::ContentBlock::Text { text } if !text.is_empty())
            });
            continue;
        };
        if let Some(message_id) = &turn.fork_message_id {
            last_turns.insert(native_turn.clone(), message_id.clone());
        }
    }
    (last_turns, unverified)
}
