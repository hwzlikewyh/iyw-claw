use std::path::Path;
use std::process::Command;
use std::time::Instant;

use iyw_claw_lib::user_memory::bench::{BenchQuery, BenchRecallMeasurement, ProductionBench};
use serde::Serialize;

use super::iyw_claw_memory_bench_report::{percentile, PUBLIC_TIMEOUT_US};

#[derive(Serialize)]
pub(crate) struct ColdProcessMeasurement {
    pub process_elapsed_us: u64,
    pub recall: BenchRecallMeasurement,
}

#[derive(Serialize)]
pub(crate) struct ColdSliceMeasurement {
    pub sample_count: usize,
    pub p50_us: Option<u64>,
    pub p95_us: Option<u64>,
    pub p99_us: Option<u64>,
    pub process_p95_us: Option<u64>,
    pub over_100ms_count: usize,
    pub at_or_over_public_timeout_count: usize,
    pub outcome_deterministic: bool,
    pub sample: ColdProcessMeasurement,
}

pub(crate) fn spawn_cold_samples(
    db_root: &Path,
    query: &BenchQuery,
    sample_count: usize,
) -> Result<Vec<ColdProcessMeasurement>, String> {
    (0..sample_count)
        .map(|_| spawn_cold(db_root, query))
        .collect()
}

pub(crate) fn cold_slice_measurement(
    samples: Vec<ColdProcessMeasurement>,
) -> Result<ColdSliceMeasurement, String> {
    let recall_latencies = samples
        .iter()
        .map(|sample| sample.recall.latency_us)
        .collect::<Vec<_>>();
    let process_latencies = samples
        .iter()
        .map(|sample| sample.process_elapsed_us)
        .collect::<Vec<_>>();
    let outcome_deterministic = cold_outcomes_deterministic(&samples);
    let over_100ms_count = recall_latencies
        .iter()
        .filter(|latency| **latency > PUBLIC_TIMEOUT_US)
        .count();
    let at_or_over_public_timeout_count = recall_latencies
        .iter()
        .filter(|latency| **latency >= PUBLIC_TIMEOUT_US)
        .count();
    let sample_count = samples.len();
    let sample = samples
        .into_iter()
        .next()
        .ok_or_else(|| "cold recall produced no fresh-process samples".to_string())?;
    Ok(ColdSliceMeasurement {
        sample_count,
        p50_us: percentile(&recall_latencies, 50),
        p95_us: percentile(&recall_latencies, 95),
        p99_us: percentile(&recall_latencies, 99),
        process_p95_us: percentile(&process_latencies, 95),
        over_100ms_count,
        at_or_over_public_timeout_count,
        outcome_deterministic,
        sample,
    })
}

pub(crate) fn spawn_cold(
    db_root: &Path,
    query: &BenchQuery,
) -> Result<ColdProcessMeasurement, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let query_json = serde_json::to_string(query).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let output = Command::new(executable)
        .arg("__cold-child")
        .arg("--db-root")
        .arg(db_root)
        .arg("--query-json")
        .arg(query_json)
        .output()
        .map_err(|error| error.to_string())?;
    let elapsed = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let recall = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "invalid cold child output: {error}; stdout={}",
            String::from_utf8_lossy(&output.stdout).trim()
        )
    })?;
    Ok(ColdProcessMeasurement {
        process_elapsed_us: elapsed,
        recall,
    })
}

pub(crate) async fn run_cold_child(db_root: &Path, query: BenchQuery) -> Result<String, String> {
    let bench = ProductionBench::open(db_root).await?;
    let measurement = bench.recall(query).await?;
    bench.close().await?;
    serde_json::to_string(&measurement).map_err(|error| error.to_string())
}

fn cold_outcomes_deterministic(samples: &[ColdProcessMeasurement]) -> bool {
    let Some(first) = samples.first().map(|sample| &sample.recall) else {
        return false;
    };
    samples.iter().all(|sample| {
        sample
            .recall
            .items
            .iter()
            .map(|item| (&item.id, &item.lanes))
            .eq(first.items.iter().map(|item| (&item.id, &item.lanes)))
            && sample.recall.abstained == first.abstained
            && sample.recall.reason_codes == first.reason_codes
    })
}
