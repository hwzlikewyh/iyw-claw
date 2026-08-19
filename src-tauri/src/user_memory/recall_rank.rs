use std::collections::{BTreeMap, BTreeSet};

use sea_orm::QueryResult;

const RRF_K: f64 = 60.0;
pub(super) const RECALL_RANKING_VERSION: &str = "rrf-v1";

#[derive(Default)]
pub(super) struct Candidate {
    score: f64,
    lanes: BTreeSet<String>,
    strong_match: bool,
}

pub(super) struct LaneScore<'a> {
    pub name: &'a str,
    pub weight: f64,
}

pub(super) fn add_rows(
    rows: Vec<QueryResult>,
    out: &mut BTreeMap<String, Candidate>,
    lane: LaneScore<'_>,
) {
    for (rank, row) in rows.into_iter().enumerate() {
        let Some(id) = row_id(&row) else { continue };
        let entry = out.entry(id).or_default();
        entry.score += lane.weight / (RRF_K + rank as f64 + 1.0);
        entry.lanes.insert(lane.name.to_string());
        entry.strong_match |= strong_lane_match(lane.name, rank);
    }
}

pub(super) fn ranked_candidates(
    candidates: BTreeMap<String, Candidate>,
) -> Vec<(String, f64, Vec<String>)> {
    let mut rows = candidates.into_iter().collect::<Vec<_>>();
    rows.retain(|(_, candidate)| candidate_is_eligible(candidate));
    rows.sort_by(|(left_id, left), (right_id, right)| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left_id.cmp(right_id))
    });
    rows.into_iter()
        .map(|(id, candidate)| (id, candidate.score, candidate.lanes.into_iter().collect()))
        .collect()
}

pub(super) fn relation_seed_ids(
    candidates: &BTreeMap<String, Candidate>,
    limit: usize,
) -> Vec<String> {
    let mut rows = candidates
        .iter()
        .filter(|(_, candidate)| candidate_is_eligible(candidate))
        .collect::<Vec<_>>();
    rows.sort_by(|(left_id, left), (right_id, right)| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left_id.cmp(right_id))
    });
    rows.into_iter()
        .take(limit)
        .map(|(id, _)| id.clone())
        .collect()
}

pub(super) fn has_lane_candidate(candidates: &BTreeMap<String, Candidate>, lanes: &[&str]) -> bool {
    candidates
        .values()
        .any(|candidate| lanes.iter().any(|lane| candidate.lanes.contains(*lane)))
}

fn candidate_is_eligible(candidate: &Candidate) -> bool {
    candidate.strong_match || candidate.lanes.len() >= 2
}

fn strong_lane_match(lane: &str, rank: usize) -> bool {
    matches!(lane, "exact" | "alias" | "temporal" | "relation")
        || matches!(lane, "unicode" | "trigram") && rank < 3
}

fn row_id(row: &QueryResult) -> Option<String> {
    row.try_get("", "id")
        .or_else(|_| row.try_get("", "memory_id"))
        .or_else(|_| row.try_get("", "target_id"))
        .ok()
}
