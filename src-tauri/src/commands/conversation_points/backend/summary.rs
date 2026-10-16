use super::super::decimal::Total;
use super::query::RequestRows;
use crate::acp::runtime_task_receipts::TaskReceipt as Receipt;
use crate::models::{
    BackendConsumption, ConfirmedConsumption, DbConversationDetail, TurnRole, TurnUsage,
};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
struct Totals {
    total: Total,
    compact: Total,
    usage: [u64; 4],
    charged: Total,
    charged_complete: bool,
    stages: Vec<crate::models::conversation::ConsumptionStage>,
    settlements: std::collections::BTreeMap<String, u64>,
}

impl Totals {
    fn add(&mut self, row: &super::query::Record, purpose: &str) -> bool {
        self.record_stage(row);
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

    fn record_stage(&mut self, row: &super::query::Record) {
        *self
            .settlements
            .entry(row.settlement_status.clone())
            .or_default() += 1;
        self.charged_complete &= row
            .charged_points
            .as_deref()
            .is_some_and(|value| self.charged.add(value).is_some());
        self.stages
            .push(crate::models::conversation::ConsumptionStage {
                request_id: row.request_id.clone(),
                stage_key: row.stage_key.clone(),
                actual_points: row.actual_points.clone(),
                charged_points: row.charged_points.clone(),
                settlement_status: row.settlement_status.clone(),
            });
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
    fn finish(self, flags: (bool, bool, bool)) -> ConfirmedConsumption {
        let (confirmed, pending, unavailable) = flags;
        let state = match (confirmed, pending, unavailable) {
            (true, false, false) => "confirmed",
            (true, _, _) => "partial",
            (false, true, _) => "pending",
            _ => "unavailable",
        };
        ConfirmedConsumption {
            account_scope: None,
            amount: confirmed.then(|| self.total.value()),
            state: state.into(),
            compaction_points: Some(self.compact.value()),
            usage: confirmed.then(|| self.usage()),
            charged_points: (self.charged_complete && !self.stages.is_empty() && !pending)
                .then(|| self.charged.value()),
            settlement_statuses: self.settlements,
            records: self.stages,
        }
    }
}

fn consumption(
    requests: &[&Receipt],
    records: &HashMap<String, RequestRows>,
) -> ConfirmedConsumption {
    let mut totals = Totals {
        charged_complete: true,
        ..Default::default()
    };
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
    totals.finish((confirmed, pending, unavailable))
}

pub(super) fn select_consumption(
    requests: &[Receipt],
    ids: &HashSet<String>,
    records: &HashMap<String, RequestRows>,
) -> ConfirmedConsumption {
    consumption(
        &requests
            .iter()
            .filter(|receipt| ids.contains(&receipt.request_id))
            .collect::<Vec<_>>(),
        records,
    )
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
