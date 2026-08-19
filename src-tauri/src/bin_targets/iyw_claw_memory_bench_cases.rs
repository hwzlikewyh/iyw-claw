use iyw_claw_lib::user_memory::bench::{
    BenchQuery, BenchRecallMeasurement, BENCH_DENSE_TEMPORAL_MONTH,
};

use super::iyw_claw_memory_bench_corpus::SyntheticMemory;

const QUERY_AT: &str = "2026-08-17T00:00:00Z";

pub(crate) struct QueryCase {
    pub query: BenchQuery,
    pub allow_ids: Vec<String>,
    pub forbid_ids: Vec<String>,
    pub expected_abstain: bool,
    pub required_lane: Option<&'static str>,
}

impl QueryCase {
    fn new(name: &str, text: String) -> Self {
        Self {
            query: BenchQuery {
                name: name.to_string(),
                text,
                query_at: QUERY_AT.to_string(),
                limit: 6,
                scope_type: "global".to_string(),
                scope_key: String::new(),
            },
            allow_ids: Vec::new(),
            forbid_ids: Vec::new(),
            expected_abstain: false,
            required_lane: None,
        }
    }

    fn allow(mut self, id: String) -> Self {
        self.allow_ids.push(id);
        self
    }

    fn forbid(mut self, id: String) -> Self {
        self.forbid_ids.push(id);
        self
    }

    fn abstain(mut self) -> Self {
        self.expected_abstain = true;
        self
    }

    fn lane(mut self, lane: &'static str) -> Self {
        self.required_lane = Some(lane);
        self
    }
}

pub(crate) fn query_cases(corpus: &[SyntheticMemory]) -> Result<Vec<QueryCase>, String> {
    let exact = item(corpus, "exact")?;
    let cjk = item(corpus, "cjk")?;
    let short = item(corpus, "short")?;
    let fts = item(corpus, "noise")?;
    let relation = item(corpus, "alias")?;
    let relation_target = relation
        .relation_ids
        .first()
        .cloned()
        .ok_or_else(|| "synthetic relation item is missing target".to_string())?;
    let temporal = item(corpus, "temporal")?;
    let conflict = item(corpus, "conflict")?;
    Ok(vec![
        QueryCase::new("exact", exact.id.clone())
            .allow(exact.id.clone())
            .lane("exact"),
        QueryCase::new("cjk", alias(cjk, 1)?)
            .allow(cjk.id.clone())
            .lane("alias"),
        QueryCase::new("short", alias(short, 0)?)
            .allow(short.id.clone())
            .lane("alias"),
        QueryCase::new("fts_union", fts.content.clone()).allow(fts.id.clone()),
        QueryCase::new("temporal", BENCH_DENSE_TEMPORAL_MONTH.to_string())
            .allow(temporal.id.clone())
            .lane("temporal"),
        QueryCase::new("relation", relation.id.clone())
            .allow(relation_target)
            .lane("relation"),
        QueryCase::new("conflict", conflict.content.clone())
            .forbid(conflict.id.clone())
            .abstain(),
        QueryCase::new("abstain", "no-such-memory-zzzxqv".to_string()).abstain(),
    ])
}

pub(crate) fn matches_expectation(case: &QueryCase, sample: &BenchRecallMeasurement) -> bool {
    let allows_present = case
        .allow_ids
        .iter()
        .all(|id| sample.items.iter().any(|item| &item.id == id));
    let forbids_absent = case
        .forbid_ids
        .iter()
        .all(|id| sample.items.iter().all(|item| &item.id != id));
    let lane_present = case.required_lane.is_none_or(|lane| {
        sample.items.iter().any(|item| {
            case.allow_ids.iter().any(|id| id == &item.id)
                && item.lanes.iter().any(|candidate| candidate == lane)
        })
    });
    allows_present && forbids_absent && lane_present && sample.abstained == case.expected_abstain
}

pub(crate) fn cold_matches_warm(
    warm: &[BenchRecallMeasurement],
    cold: &BenchRecallMeasurement,
) -> bool {
    warm.first().is_some_and(|sample| {
        sample
            .items
            .iter()
            .map(|item| (&item.id, &item.lanes))
            .eq(cold.items.iter().map(|item| (&item.id, &item.lanes)))
            && sample.abstained == cold.abstained
            && sample.reason_codes == cold.reason_codes
    })
}

fn item<'a>(corpus: &'a [SyntheticMemory], kind: &str) -> Result<&'a SyntheticMemory, String> {
    corpus
        .iter()
        .find(|item| item.kind == kind)
        .ok_or_else(|| format!("synthetic corpus is missing {kind} item"))
}

fn alias(item: &SyntheticMemory, index: usize) -> Result<String, String> {
    item.aliases
        .get(index)
        .cloned()
        .ok_or_else(|| format!("synthetic {} item is missing alias {index}", item.kind))
}
