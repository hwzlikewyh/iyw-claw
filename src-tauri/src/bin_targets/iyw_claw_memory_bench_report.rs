use serde::Serialize;
use std::collections::BTreeMap;

use iyw_claw_lib::user_memory::bench::{
    BenchBuildMetrics, BenchQueryPlan, BenchRecallMeasurement, BenchStorageMetrics,
};

use super::iyw_claw_memory_bench_process::ColdSliceMeasurement;
use super::iyw_claw_memory_bench_public::PublicServiceMeasurement;

pub(crate) const PUBLIC_TIMEOUT_US: u64 = 100_000;

#[derive(Serialize)]
pub(crate) struct BenchReport {
    pub status: &'static str,
    pub schema_version: &'static str,
    pub generator_version: &'static str,
    pub query_plan_version: &'static str,
    pub ranking_version: &'static str,
    pub corpus: CorpusReport,
    pub environment: EnvironmentReport,
    pub methods: MethodsReport,
    pub rebuild: BenchBuildMetrics,
    pub storage: BenchStorageMetrics,
    pub production_recall: ProductionRecallReport,
    pub query_plans: Vec<BenchQueryPlan>,
    pub plan_validation: PlanValidation,
    pub required_matrix: Matrix,
}

#[derive(Serialize)]
pub(crate) struct CorpusReport {
    pub size: usize,
    pub seed: u64,
    pub digest: String,
    pub fixture_digests: BTreeMap<String, String>,
    pub output: String,
}

#[derive(Serialize)]
pub(crate) struct EnvironmentReport {
    pub git_commit: String,
    pub git_dirty: String,
    pub build_profile: &'static str,
    pub os: String,
    pub arch: String,
    pub cpu: String,
    pub sqlite_fts_capability: String,
    pub connection_pool: &'static str,
}

#[derive(Serialize)]
pub(crate) struct MethodsReport {
    pub cold: &'static str,
    pub warm: &'static str,
    pub network: &'static str,
    pub llm: &'static str,
    pub adapter: &'static str,
    pub limitations: [&'static str; 5],
}

#[derive(Serialize)]
pub(crate) struct ProductionRecallReport {
    pub status: &'static str,
    pub correctness_status: &'static str,
    pub performance_gate: PerformanceGate,
    pub public_service: PublicServiceMeasurement,
    pub queries: Vec<QueryMeasurement>,
}

#[derive(Serialize)]
pub(crate) struct PerformanceGate {
    pub status: &'static str,
    pub latency_gate_applies: bool,
    pub timeout_safety_applies: bool,
    pub corpus_size: usize,
    pub warm_p95_limit_us: u64,
    pub warm_max_slice_p95_us: Option<u64>,
    pub cold_p95_limit_us: u64,
    pub cold_max_slice_p95_us: Option<u64>,
    pub cold_process_max_slice_p95_us: Option<u64>,
    pub end_to_end_p95_limit_us: u64,
    pub end_to_end_p95_us: Option<u64>,
    pub public_timeout_us: u64,
    pub public_timeout_wrapper_measured: bool,
    pub observed_recall_count: usize,
    pub over_100ms_count: usize,
    pub at_or_over_public_timeout_count: usize,
    pub failed_checks: Vec<String>,
    pub unmeasured_checks: Vec<&'static str>,
}

#[derive(Serialize)]
pub(crate) struct SuiteReport {
    pub schema_version: &'static str,
    pub status: &'static str,
    pub quality_report: SuiteReportEntry,
    pub performance_reports: Vec<SuiteReportEntry>,
}

#[derive(Serialize)]
pub(crate) struct SuiteReportEntry {
    pub name: String,
    pub status: String,
    pub path: String,
}

#[derive(Serialize)]
pub(crate) struct QueryMeasurement {
    pub name: String,
    pub warm: WarmMeasurement,
    pub cold: ColdSliceMeasurement,
    pub allow_ids: Vec<String>,
    pub forbid_ids: Vec<String>,
    pub expected_abstain: bool,
    pub required_lane: Option<&'static str>,
    pub expectation_met: bool,
    pub sample: BenchRecallMeasurement,
}

#[derive(Serialize)]
pub(crate) struct WarmMeasurement {
    pub iterations: usize,
    pub p50_us: Option<u64>,
    pub p95_us: Option<u64>,
    pub p99_us: Option<u64>,
    pub candidate_sql_count: usize,
    pub hydrate_sql_count: usize,
    pub total_sql_count: usize,
    pub sql_counts_constant: bool,
    pub hydrate_constant_for_non_empty: bool,
    pub outcome_deterministic: bool,
    pub over_100ms_count: usize,
    pub at_or_over_public_timeout_count: usize,
}

#[derive(Serialize)]
pub(crate) struct PlanValidation {
    pub status: &'static str,
    pub temporal_index: &'static str,
    pub temporal_index_hit: bool,
    pub missing_plan_lanes: Vec<&'static str>,
}

#[derive(Serialize)]
pub(crate) struct Matrix {
    pub active_items: [&'static str; 3],
    pub query_slices: [&'static str; 8],
    pub cache_states: [&'static str; 2],
    pub execution_modes: [&'static str; 2],
}

pub(crate) fn warm_measurement(samples: &[BenchRecallMeasurement]) -> WarmMeasurement {
    let latencies = samples
        .iter()
        .map(|sample| sample.latency_us)
        .collect::<Vec<_>>();
    let first = samples.first();
    WarmMeasurement {
        iterations: samples.len(),
        p50_us: percentile(&latencies, 50),
        p95_us: percentile(&latencies, 95),
        p99_us: percentile(&latencies, 99),
        candidate_sql_count: first.map_or(0, |sample| sample.candidate_sql_count),
        hydrate_sql_count: first.map_or(0, |sample| sample.hydrate_sql_count),
        total_sql_count: first.map_or(0, |sample| sample.total_sql_count),
        sql_counts_constant: constant_sql_counts(samples),
        hydrate_constant_for_non_empty: constant_non_empty_hydrate(samples),
        outcome_deterministic: deterministic_outcomes(samples),
        over_100ms_count: samples
            .iter()
            .filter(|sample| sample.latency_us > PUBLIC_TIMEOUT_US)
            .count(),
        at_or_over_public_timeout_count: samples
            .iter()
            .filter(|sample| sample.latency_us >= PUBLIC_TIMEOUT_US)
            .count(),
    }
}

pub(crate) fn plan_validation(plans: &[BenchQueryPlan]) -> PlanValidation {
    let required = ["stable", "alias", "fts_unicode", "fts_trigram", "hydrate"];
    let missing = required
        .into_iter()
        .filter(|lane| {
            !plans
                .iter()
                .any(|plan| plan.lane == *lane && plan.error.is_none())
        })
        .collect::<Vec<_>>();
    let temporal_hit = plans.iter().any(|plan| {
        plan.lane == "temporal" && plan.required_index_hit == Some(true) && plan.error.is_none()
    });
    PlanValidation {
        status: if temporal_hit && missing.is_empty() {
            "passed"
        } else {
            "failed"
        },
        temporal_index: "idx_memory_evidence_time",
        temporal_index_hit: temporal_hit,
        missing_plan_lanes: missing,
    }
}

pub(crate) fn percentile(values: &[u64], percentile: usize) -> Option<u64> {
    let mut values = values.to_vec();
    values.sort_unstable();
    let last = values.len().checked_sub(1)?;
    values.get((last * percentile + 99) / 100).copied()
}

fn constant_sql_counts(samples: &[BenchRecallMeasurement]) -> bool {
    let Some(first) = samples.first() else {
        return false;
    };
    samples.iter().all(|sample| {
        sample.candidate_sql_count == first.candidate_sql_count
            && sample.hydrate_sql_count == first.hydrate_sql_count
            && sample.total_sql_count == first.total_sql_count
    })
}

fn constant_non_empty_hydrate(samples: &[BenchRecallMeasurement]) -> bool {
    samples
        .iter()
        .filter(|sample| !sample.abstained)
        .all(|sample| sample.hydrate_sql_count == 1)
}

fn deterministic_outcomes(samples: &[BenchRecallMeasurement]) -> bool {
    let Some(first) = samples.first() else {
        return false;
    };
    samples.iter().all(|sample| {
        sample
            .items
            .iter()
            .map(|item| (&item.id, &item.lanes))
            .collect::<Vec<_>>()
            == first
                .items
                .iter()
                .map(|item| (&item.id, &item.lanes))
                .collect::<Vec<_>>()
            && sample.abstained == first.abstained
            && sample.reason_codes == first.reason_codes
    })
}
