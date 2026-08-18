use std::sync::OnceLock;
use std::time::Instant;

use serde::Serialize;

use super::recall_rank::RECALL_RANKING_VERSION;
use super::recall_types::UserMemoryRecallItem;

const SCORE_DIRECTION: &str = "higher_score_is_better";
const RECALL_SHADOW_ENV: &str = "IYW_CLAW_USER_MEMORY_RECALL_SHADOW";

#[derive(Clone, Debug, Serialize)]
struct LaneShadow {
    lane: &'static str,
    candidate_count: usize,
    filter_count: usize,
    contribution_count: usize,
    latency_ms: u64,
    empty_reason: &'static str,
    score_direction: &'static str,
}

#[derive(Clone)]
pub(super) struct RecallShadow {
    started_at: Instant,
    lanes: Vec<LaneShadow>,
    union_count: usize,
    ranked_count: usize,
}

pub(super) struct LaneMeasurement {
    lane: &'static str,
    started_at: Instant,
    candidate_count: usize,
    empty_reason: Option<&'static str>,
    score_direction: &'static str,
}

impl LaneMeasurement {
    pub(super) fn collected(
        lane: &'static str,
        started_at: Instant,
        candidate_count: usize,
    ) -> Self {
        Self {
            lane,
            started_at,
            candidate_count,
            empty_reason: (candidate_count == 0).then_some("no_candidates"),
            score_direction: SCORE_DIRECTION,
        }
    }

    pub(super) fn empty(lane: &'static str, started_at: Instant, reason: &'static str) -> Self {
        Self {
            lane,
            started_at,
            candidate_count: 0,
            empty_reason: Some(reason),
            score_direction: SCORE_DIRECTION,
        }
    }

    pub(super) fn with_reason(mut self, reason: Option<&'static str>) -> Self {
        if reason.is_some() {
            self.empty_reason = reason;
        }
        self
    }

    pub(super) fn without_score(mut self) -> Self {
        self.score_direction = "not_applicable";
        self
    }
}

impl RecallShadow {
    pub(super) fn new(started_at: Instant) -> Self {
        Self {
            started_at,
            lanes: Vec::new(),
            union_count: 0,
            ranked_count: 0,
        }
    }

    pub(super) fn record_lane(&mut self, measurement: LaneMeasurement) {
        self.lanes.push(LaneShadow {
            lane: measurement.lane,
            candidate_count: measurement.candidate_count,
            filter_count: 0,
            contribution_count: 0,
            latency_ms: elapsed_ms(measurement.started_at),
            empty_reason: measurement.empty_reason.unwrap_or("none"),
            score_direction: measurement.score_direction,
        });
    }

    pub(super) fn set_ranking_counts(&mut self, union_count: usize, ranked_count: usize) {
        self.union_count = union_count;
        self.ranked_count = ranked_count;
    }

    pub(super) fn log(mut self, mode: &'static str, items: &[UserMemoryRecallItem], reason: &str) {
        if !recall_shadow_enabled() {
            return;
        }
        self.set_contributions(items);
        let lane_metrics = serde_json::to_string(&self.lanes)
            .unwrap_or_else(|_| "lane_metrics_unavailable".to_string());
        let candidate_sum = self.candidate_sum_for_mode(mode);
        tracing::info!(
            target: "memory_recall_shadow",
            ranking_version = RECALL_RANKING_VERSION,
            mode,
            lane_metrics,
            union_count = self.union_count,
            deduplicated_count = candidate_sum.saturating_sub(self.union_count),
            ranked_count = self.ranked_count,
            final_count = items.len(),
            abstained = items.is_empty(),
            abstain_reason = reason,
            total_latency_ms = elapsed_ms(self.started_at),
            "[memory-recall-shadow] recall decision"
        );
    }

    fn set_contributions(&mut self, items: &[UserMemoryRecallItem]) {
        for lane in &mut self.lanes {
            lane.contribution_count = if lane.lane == "hydrate" {
                items.len()
            } else {
                items
                    .iter()
                    .filter(|item| item.lanes.iter().any(|name| name == lane.lane))
                    .count()
            };
            lane.filter_count = lane.candidate_count.saturating_sub(lane.contribution_count);
        }
    }

    fn candidate_sum_for_mode(&self, mode: &str) -> usize {
        self.lanes
            .iter()
            .filter(|lane| match mode {
                "index" => lane.lane != "hydrate" && !lane.lane.starts_with("source_"),
                "source_fallback" => lane.lane.starts_with("source_"),
                _ => false,
            })
            .map(|lane| lane.candidate_count)
            .sum()
    }
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis().min(u64::MAX as u128) as u64
}

fn recall_shadow_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var(RECALL_SHADOW_ENV)
            .ok()
            .as_deref()
            .map(parse_enabled)
            .unwrap_or(true)
    })
}

fn parse_enabled(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}
