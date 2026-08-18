use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use iyw_claw_lib::user_memory::bench::{BenchMemoryInput, BenchQuery, ProductionBench};

use super::iyw_claw_memory_bench_cases::{
    cold_matches_warm, matches_expectation, query_cases, QueryCase,
};
use super::iyw_claw_memory_bench_config::{
    corpus_file_name, output_dir, parse_cold_args, parse_config, parse_suite_seed,
    report_file_name, suite_report_file_name, write_file, Config,
};
use super::iyw_claw_memory_bench_corpus::{
    digest, generate_corpus, serialize_corpus, SyntheticMemory,
};
use super::iyw_claw_memory_bench_metadata::{add_fts_limitations, assemble_report, ReportParts};
use super::iyw_claw_memory_bench_process::{
    cold_slice_measurement, run_cold_child, spawn_cold_samples,
};
use super::iyw_claw_memory_bench_public::measure_public_service;
use super::iyw_claw_memory_bench_quality::write_quality_report;
use super::iyw_claw_memory_bench_report::{
    warm_measurement, QueryMeasurement, SuiteReport, SuiteReportEntry,
};

const WARM_ITERATIONS: usize = 30;
const COLD_PROCESS_SAMPLES: usize = 5;

struct ReportOutcome {
    path: PathBuf,
    status: &'static str,
    size: usize,
}

pub(crate) struct CommandOutcome {
    pub message: String,
    pub passed: bool,
}

impl CommandOutcome {
    fn passed(message: String) -> Self {
        Self {
            message,
            passed: true,
        }
    }

    fn measured(message: String, status: &str) -> Self {
        Self {
            message,
            passed: status == "passed",
        }
    }
}

pub async fn run(args: Vec<String>) -> Result<CommandOutcome, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err("missing command".to_string());
    };
    match command {
        "generate" => generate(parse_config(&args[1..])?).map(CommandOutcome::passed),
        "report" => report(parse_config(&args[1..])?).await,
        "quality" => quality().await,
        "suite" => suite(parse_suite_seed(&args[1..])?).await,
        "__cold-child" => cold_child(&args[1..]).await.map(CommandOutcome::passed),
        _ => Err(format!("unsupported command: {command}")),
    }
}

pub fn usage() -> &'static str {
    "usage: iyw-claw-memory-bench <generate|report|quality|suite> [--size 1000|10000|50000] [--seed N]"
}

fn generate(config: Config) -> Result<String, String> {
    let corpus = generate_corpus(config.size, config.seed);
    let jsonl = serialize_corpus(&corpus)?;
    let path = output_dir().join(corpus_file_name(&config));
    write_file(&path, jsonl.as_bytes())?;
    Ok(format!("wrote {} items to {}", config.size, path.display()))
}

async fn report(config: Config) -> Result<CommandOutcome, String> {
    let outcome = run_report(config).await?;
    let message = format!(
        "wrote production benchmark report to {}; status={}",
        outcome.path.display(),
        outcome.status
    );
    Ok(CommandOutcome::measured(message, outcome.status))
}

async fn quality() -> Result<CommandOutcome, String> {
    let path = write_quality_report().await?;
    let status = report_entry("quality", &path)?.status;
    let message = format!(
        "wrote production recall quality report to {}",
        path.display()
    );
    Ok(CommandOutcome::measured(message, &status))
}

async fn suite(seed: u64) -> Result<CommandOutcome, String> {
    let quality_path = write_quality_report().await?;
    let quality = report_entry("quality", &quality_path)?;
    let mut performance = Vec::new();
    for size in [1_000, 10_000, 50_000] {
        performance.push(run_report(Config { size, seed }).await?);
    }
    let status = super::iyw_claw_memory_bench_gates::aggregate_status(
        std::iter::once(quality.status.as_str()).chain(performance.iter().map(|item| item.status)),
    );
    let report = SuiteReport {
        schema_version: "MemoryRecallBenchSuiteV1",
        status,
        quality_report: quality,
        performance_reports: performance.iter().map(outcome_entry).collect(),
    };
    let path = output_dir().join(suite_report_file_name(seed));
    let encoded = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    write_file(&path, &encoded)?;
    let paths = performance
        .iter()
        .map(|outcome| outcome.path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let message = format!(
        "wrote benchmark suite to {}; status={status}; reports={paths}",
        path.display()
    );
    Ok(CommandOutcome::measured(message, status))
}

async fn run_report(config: Config) -> Result<ReportOutcome, String> {
    fs::create_dir_all(output_dir()).map_err(|error| error.to_string())?;
    let corpus = generate_corpus(config.size, config.seed);
    let jsonl = serialize_corpus(&corpus)?;
    let corpus_path = output_dir().join(corpus_file_name(&config));
    write_file(&corpus_path, jsonl.as_bytes())?;
    let temp = tempfile::Builder::new()
        .prefix("memory-bench-db-")
        .tempdir_in(output_dir())
        .map_err(|error| error.to_string())?;
    let inputs = bench_inputs(&corpus);
    let (bench, rebuild) = ProductionBench::create(temp.path(), digest(&jsonl), inputs).await?;
    let queries = measure_queries(&bench, temp.path(), query_cases(&corpus)?).await?;
    let public_service = measure_public_service(config.size, &output_dir()).await?;
    let mut plans = bench.query_plans().await;
    let status = bench.index_status().await?;
    add_fts_limitations(&mut plans, &status);
    let storage = bench.storage_metrics().await;
    bench.close().await?;
    let report = assemble_report(ReportParts {
        config: &config,
        jsonl: &jsonl,
        corpus_path: &corpus_path,
        rebuild,
        storage,
        status,
        queries,
        public_service,
        plans,
    });
    let report_path = output_dir().join(report_file_name(&config));
    let encoded = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    write_file(&report_path, &encoded)?;
    let status = report.status;
    drop(temp);
    Ok(ReportOutcome {
        path: report_path,
        status,
        size: config.size,
    })
}

fn report_entry(name: &str, path: &Path) -> Result<SuiteReportEntry, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let value =
        serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|error| error.to_string())?;
    let status = value
        .get("status")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("report {} has no string status", path.display()))?;
    Ok(SuiteReportEntry {
        name: name.to_string(),
        status: status.to_string(),
        path: path.display().to_string(),
    })
}

fn outcome_entry(outcome: &ReportOutcome) -> SuiteReportEntry {
    SuiteReportEntry {
        name: format!("performance_{}", outcome.size),
        status: outcome.status.to_string(),
        path: outcome.path.display().to_string(),
    }
}

async fn measure_queries(
    bench: &ProductionBench,
    db_root: &Path,
    cases: Vec<QueryCase>,
) -> Result<Vec<QueryMeasurement>, String> {
    let mut measurements = Vec::with_capacity(cases.len());
    for case in cases {
        let samples = warm_samples(bench, &case.query).await?;
        let sample = samples
            .last()
            .cloned()
            .ok_or_else(|| "warm recall produced no samples".to_string())?;
        let cold_samples = spawn_cold_samples(db_root, &case.query, COLD_PROCESS_SAMPLES)?;
        let expectation_met = samples
            .iter()
            .chain(cold_samples.iter().map(|sample| &sample.recall))
            .all(|value| matches_expectation(&case, value))
            && cold_samples
                .iter()
                .all(|sample| cold_matches_warm(&samples, &sample.recall));
        let cold = cold_slice_measurement(cold_samples)?;
        measurements.push(QueryMeasurement {
            name: case.query.name,
            warm: warm_measurement(&samples),
            cold,
            allow_ids: case.allow_ids,
            forbid_ids: case.forbid_ids,
            expected_abstain: case.expected_abstain,
            required_lane: case.required_lane,
            expectation_met,
            sample,
        });
    }
    Ok(measurements)
}

async fn warm_samples(
    bench: &ProductionBench,
    query: &BenchQuery,
) -> Result<Vec<iyw_claw_lib::user_memory::bench::BenchRecallMeasurement>, String> {
    let mut samples = Vec::with_capacity(WARM_ITERATIONS);
    for _ in 0..WARM_ITERATIONS {
        samples.push(bench.recall(query.clone()).await?);
    }
    Ok(samples)
}

fn bench_inputs(corpus: &[SyntheticMemory]) -> Vec<BenchMemoryInput> {
    let mut groups = BTreeMap::<&str, Vec<&str>>::new();
    for item in corpus {
        if let Some(group) = item.conflict_group.as_deref() {
            groups.entry(group).or_default().push(&item.id);
        }
    }
    corpus
        .iter()
        .map(|item| bench_input(item, &groups))
        .collect()
}

fn bench_input(item: &SyntheticMemory, groups: &BTreeMap<&str, Vec<&str>>) -> BenchMemoryInput {
    let contradicts_ids = item
        .conflict_group
        .as_deref()
        .and_then(|group| groups.get(group))
        .into_iter()
        .flatten()
        .filter(|target| **target != item.id)
        .map(|target| (*target).to_string())
        .collect();
    BenchMemoryInput {
        id: item.id.clone(),
        kind: item.kind.clone(),
        content: item.content.clone(),
        content_digest: item.content_digest.clone(),
        aliases: item.aliases.clone(),
        scope_type: item.scope.r#type.clone(),
        scope_key: item.scope.key.clone(),
        sensitive: item.sensitive,
        superseded_by: None,
        source_revision: item.source_revision.clone(),
        valid_from: Some(item.valid_from.clone()),
        valid_to: item.valid_to.clone(),
        relation_ids: item.relation_ids.clone(),
        contradicts_ids,
    }
}

async fn cold_child(args: &[String]) -> Result<String, String> {
    let (root, query) = parse_cold_args(args)?;
    run_cold_child(&root, query).await
}
