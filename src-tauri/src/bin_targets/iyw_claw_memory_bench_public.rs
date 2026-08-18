use std::fs;
use std::path::Path;

use iyw_claw_lib::user_memory::bench::public_recall::{PublicRecallBench, PublicRecallSample};
use serde::Serialize;

use super::iyw_claw_memory_bench_report::{percentile, PUBLIC_TIMEOUT_US};

const GATED_CORPUS_SIZE: usize = 10_000;
const PUBLIC_WARM_ITERATIONS: usize = 30;
const PUBLIC_EXACT_ID: &str = "iyw-memory-bench-public-exact";

#[derive(Serialize)]
pub(crate) struct PublicServiceMeasurement {
    pub status: &'static str,
    pub applies: bool,
    pub source_item_count: usize,
    pub iterations: usize,
    pub p50_us: Option<u64>,
    pub p95_us: Option<u64>,
    pub p99_us: Option<u64>,
    pub timeout_count: usize,
    pub over_100ms_count: usize,
    pub at_or_over_public_timeout_count: usize,
    pub expected_exact_id: &'static str,
    pub expected_exact_hit: bool,
    pub ready_indexed_outcome: bool,
    pub outcome_deterministic: bool,
    pub representative_item_ids: Vec<String>,
}

pub(crate) async fn measure_public_service(
    size: usize,
    output_dir: &Path,
) -> Result<PublicServiceMeasurement, String> {
    if size != GATED_CORPUS_SIZE {
        return Ok(not_applicable());
    }
    let temp = tempfile::Builder::new()
        .prefix("memory-public-recall-")
        .tempdir_in(output_dir)
        .map_err(|error| error.to_string())?;
    let measurement = measure_in_temp(temp.path(), size).await;
    let cleanup = temp.close().map_err(|error| error.to_string());
    match (measurement, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(format!("public recall temp cleanup failed: {error}")),
    }
}

async fn measure_in_temp(root: &Path, size: usize) -> Result<PublicServiceMeasurement, String> {
    let source_root = root.join("source");
    let db_root = root.join("database");
    write_truth_source(&source_root, size)?;
    let bench = PublicRecallBench::create(&db_root, &source_root).await?;
    let measurement = collect_samples(&bench, size).await;
    let close = bench.close().await;
    match (measurement, close) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(format!("public recall database close failed: {error}")),
    }
}

async fn collect_samples(
    bench: &PublicRecallBench,
    size: usize,
) -> Result<PublicServiceMeasurement, String> {
    let source_item_count = bench.indexed_item_count().await?;
    if source_item_count != size {
        return Err(format!(
            "public recall indexed {source_item_count} source items; expected {size}"
        ));
    }
    let mut samples = Vec::with_capacity(PUBLIC_WARM_ITERATIONS);
    for _ in 0..PUBLIC_WARM_ITERATIONS {
        samples.push(bench.recall(PUBLIC_EXACT_ID).await?);
    }
    Ok(summarize(source_item_count, &samples))
}

fn summarize(source_item_count: usize, samples: &[PublicRecallSample]) -> PublicServiceMeasurement {
    let latencies = samples
        .iter()
        .map(|sample| sample.latency_us)
        .collect::<Vec<_>>();
    let expected_exact_hit = samples
        .iter()
        .all(|sample| sample.item_ids.iter().any(|id| id == PUBLIC_EXACT_ID));
    let outcome_deterministic = deterministic(samples);
    let ready_indexed_outcome = samples.iter().all(|sample| {
        sample.status == "ready" && sample.index_generation.is_some() && !sample.abstained
    });
    let timeout_count = samples.iter().filter(|sample| timed_out(sample)).count();
    let over_100ms_count = samples
        .iter()
        .filter(|sample| sample.latency_us > PUBLIC_TIMEOUT_US)
        .count();
    let at_or_over_public_timeout_count = samples
        .iter()
        .filter(|sample| sample.latency_us >= PUBLIC_TIMEOUT_US)
        .count();
    PublicServiceMeasurement {
        status: if expected_exact_hit && ready_indexed_outcome && outcome_deterministic {
            "passed"
        } else {
            "failed"
        },
        applies: true,
        source_item_count,
        iterations: samples.len(),
        p50_us: percentile(&latencies, 50),
        p95_us: percentile(&latencies, 95),
        p99_us: percentile(&latencies, 99),
        timeout_count,
        over_100ms_count,
        at_or_over_public_timeout_count,
        expected_exact_id: PUBLIC_EXACT_ID,
        expected_exact_hit,
        ready_indexed_outcome,
        outcome_deterministic,
        representative_item_ids: samples
            .first()
            .map(|sample| sample.item_ids.clone())
            .unwrap_or_default(),
    }
}

fn deterministic(samples: &[PublicRecallSample]) -> bool {
    let Some(first) = samples.first() else {
        return false;
    };
    samples.iter().all(|sample| {
        sample.item_ids == first.item_ids
            && sample.status == first.status
            && sample.index_generation == first.index_generation
            && sample.abstained == first.abstained
            && sample.reason_codes == first.reason_codes
    })
}

fn timed_out(sample: &PublicRecallSample) -> bool {
    sample.status == "timeout"
        || sample
            .reason_codes
            .iter()
            .any(|reason| reason == "recall_timeout")
}

fn write_truth_source(root: &Path, size: usize) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let memory = format!("Public recall benchmark exact truth <!-- {PUBLIC_EXACT_ID} -->\n");
    fs::write(root.join("user-memory.md"), memory).map_err(|error| error.to_string())?;
    fs::write(root.join("user-profile.md"), profile_fillers(size - 1))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn profile_fillers(count: usize) -> String {
    (0..count)
        .map(|index| index.to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn not_applicable() -> PublicServiceMeasurement {
    PublicServiceMeasurement {
        status: "not_applicable",
        applies: false,
        source_item_count: 0,
        iterations: 0,
        p50_us: None,
        p95_us: None,
        p99_us: None,
        timeout_count: 0,
        over_100ms_count: 0,
        at_or_over_public_timeout_count: 0,
        expected_exact_id: PUBLIC_EXACT_ID,
        expected_exact_hit: false,
        ready_indexed_outcome: false,
        outcome_deterministic: false,
        representative_item_ids: Vec::new(),
    }
}
