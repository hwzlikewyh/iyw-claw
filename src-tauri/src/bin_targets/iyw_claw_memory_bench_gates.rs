use super::iyw_claw_memory_bench_public::PublicServiceMeasurement;
use super::iyw_claw_memory_bench_report::{PerformanceGate, QueryMeasurement, PUBLIC_TIMEOUT_US};

const GATED_CORPUS_SIZE: usize = 10_000;
const WARM_P95_LIMIT_US: u64 = 50_000;
const COLD_P95_LIMIT_US: u64 = 100_000;
const END_TO_END_P95_LIMIT_US: u64 = 100_000;

pub(crate) fn performance_gate(
    size: usize,
    queries: &[QueryMeasurement],
    public: &PublicServiceMeasurement,
) -> PerformanceGate {
    let warm_p95 = queries.iter().filter_map(|query| query.warm.p95_us).max();
    let cold_p95 = queries.iter().filter_map(|query| query.cold.p95_us).max();
    let cold_process_p95 = queries
        .iter()
        .filter_map(|query| query.cold.process_p95_us)
        .max();
    let latency_gate_applies = size == GATED_CORPUS_SIZE;
    let timeout_exposure = timeout_exposure_count(queries, public);
    let mut failed_checks =
        latency_failures(latency_gate_applies, warm_p95, cold_p95, public.p95_us);
    add_timeout_failure(&mut failed_checks, timeout_exposure);
    let unmeasured_checks = unmeasured_checks(latency_gate_applies, queries, public);
    PerformanceGate {
        status: gate_status(&failed_checks, &unmeasured_checks),
        latency_gate_applies,
        timeout_safety_applies: true,
        corpus_size: size,
        warm_p95_limit_us: WARM_P95_LIMIT_US,
        warm_max_slice_p95_us: warm_p95,
        cold_p95_limit_us: COLD_P95_LIMIT_US,
        cold_max_slice_p95_us: cold_p95,
        cold_process_max_slice_p95_us: cold_process_p95,
        end_to_end_p95_limit_us: END_TO_END_P95_LIMIT_US,
        end_to_end_p95_us: public.p95_us,
        public_timeout_us: PUBLIC_TIMEOUT_US,
        public_timeout_wrapper_measured: public.applies && public.iterations > 0,
        observed_recall_count: observed_recall_count(queries, public),
        over_100ms_count: over_100ms_count(queries, public),
        at_or_over_public_timeout_count: timeout_exposure,
        failed_checks,
        unmeasured_checks,
    }
}

pub(crate) fn aggregate_status<'a>(statuses: impl IntoIterator<Item = &'a str>) -> &'static str {
    let statuses = statuses.into_iter().collect::<Vec<_>>();
    if statuses.iter().any(|status| *status == "failed") {
        "failed"
    } else if statuses
        .iter()
        .any(|status| !matches!(*status, "passed" | "not_applicable"))
    {
        "incomplete"
    } else {
        "passed"
    }
}

fn latency_failures(
    applies: bool,
    warm_p95: Option<u64>,
    cold_p95: Option<u64>,
    public_p95: Option<u64>,
) -> Vec<String> {
    if !applies {
        return Vec::new();
    }
    let mut failures = Vec::new();
    add_latency_failure(
        &mut failures,
        "warm_max_slice_p95_us",
        warm_p95,
        WARM_P95_LIMIT_US,
    );
    add_latency_failure(
        &mut failures,
        "cold_max_slice_p95_us",
        cold_p95,
        COLD_P95_LIMIT_US,
    );
    add_latency_failure(
        &mut failures,
        "public_service_p95_us",
        public_p95,
        END_TO_END_P95_LIMIT_US,
    );
    failures
}

fn add_timeout_failure(failures: &mut Vec<String>, timeout_exposure: usize) {
    if timeout_exposure > 0 {
        failures.push(format!(
            "{timeout_exposure} observed recalls reached the {PUBLIC_TIMEOUT_US}us timeout budget"
        ));
    }
}

fn add_latency_failure(failures: &mut Vec<String>, name: &str, observed: Option<u64>, limit: u64) {
    match observed {
        Some(value) if value <= limit => {}
        Some(value) => failures.push(format!("{name}={value} exceeds {limit}")),
        None => {}
    }
}

fn gate_status(failed_checks: &[String], unmeasured_checks: &[&str]) -> &'static str {
    if !failed_checks.is_empty() {
        "failed"
    } else if !unmeasured_checks.is_empty() {
        "incomplete"
    } else {
        "passed"
    }
}

fn unmeasured_checks(
    applies: bool,
    queries: &[QueryMeasurement],
    public: &PublicServiceMeasurement,
) -> Vec<&'static str> {
    if !applies {
        return Vec::new();
    }
    let mut missing = Vec::new();
    if cfg!(debug_assertions) {
        missing.push("release build profile");
    }
    if !public.applies || public.iterations == 0 || public.p95_us.is_none() {
        missing.push("public UserMemoryService::recall p95");
    }
    if queries
        .iter()
        .any(|query| query.cold.sample_count < 5 || query.cold.p95_us.is_none())
    {
        missing.push("per-slice cold p95 from at least five fresh processes");
    }
    missing
}

fn observed_recall_count(queries: &[QueryMeasurement], public: &PublicServiceMeasurement) -> usize {
    queries
        .iter()
        .map(|query| query.warm.iterations + query.cold.sample_count)
        .sum::<usize>()
        + public.iterations
}

fn over_100ms_count(queries: &[QueryMeasurement], public: &PublicServiceMeasurement) -> usize {
    let warm = queries
        .iter()
        .map(|query| query.warm.over_100ms_count)
        .sum::<usize>();
    warm + queries
        .iter()
        .map(|query| query.cold.over_100ms_count)
        .sum::<usize>()
        + public.over_100ms_count
}

fn timeout_exposure_count(
    queries: &[QueryMeasurement],
    public: &PublicServiceMeasurement,
) -> usize {
    let warm = queries
        .iter()
        .map(|query| query.warm.at_or_over_public_timeout_count)
        .sum::<usize>();
    warm + queries
        .iter()
        .map(|query| query.cold.at_or_over_public_timeout_count)
        .sum::<usize>()
        + public.at_or_over_public_timeout_count
}
