use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use iyw_claw_lib::user_memory::bench::{BenchBuildMetrics, BenchQueryPlan, BenchStorageMetrics};
use iyw_claw_lib::user_memory::UserMemoryIndexStatus;

use super::iyw_claw_memory_bench_config::Config;
use super::iyw_claw_memory_bench_corpus::{digest, digest_bytes};
use super::iyw_claw_memory_bench_gates::{aggregate_status, performance_gate};
use super::iyw_claw_memory_bench_public::PublicServiceMeasurement;
use super::iyw_claw_memory_bench_report::{
    plan_validation, BenchReport, CorpusReport, EnvironmentReport, Matrix, MethodsReport,
    ProductionRecallReport, QueryMeasurement,
};

pub(crate) struct ReportParts<'a> {
    pub config: &'a Config,
    pub jsonl: &'a str,
    pub corpus_path: &'a Path,
    pub rebuild: BenchBuildMetrics,
    pub storage: BenchStorageMetrics,
    pub status: UserMemoryIndexStatus,
    pub queries: Vec<QueryMeasurement>,
    pub public_service: PublicServiceMeasurement,
    pub plans: Vec<BenchQueryPlan>,
}

pub(crate) fn assemble_report(parts: ReportParts<'_>) -> BenchReport {
    let query_correctness = parts.queries.iter().all(|query| {
        query.expectation_met
            && query.warm.sql_counts_constant
            && query.warm.hydrate_constant_for_non_empty
            && query.warm.outcome_deterministic
            && query.cold.outcome_deterministic
    });
    let public_correctness = !parts.public_service.applies
        || parts.public_service.expected_exact_hit
            && parts.public_service.ready_indexed_outcome
            && parts.public_service.outcome_deterministic;
    let correctness_status = if query_correctness && public_correctness {
        "passed"
    } else {
        "failed"
    };
    let performance_gate =
        performance_gate(parts.config.size, &parts.queries, &parts.public_service);
    let recall_status = aggregate_status([correctness_status, performance_gate.status]);
    let plan_validation = plan_validation(&parts.plans);
    let report_status = aggregate_status([recall_status, plan_validation.status]);
    BenchReport {
        status: report_status,
        schema_version: "MemoryRecallBenchReportV4",
        generator_version: "memory-bench-synth-v3",
        query_plan_version: "tier1-recall-v1",
        ranking_version: "rrf-v1",
        corpus: corpus_report(parts.config, parts.jsonl, parts.corpus_path),
        environment: environment(&parts.status),
        methods: methods(),
        rebuild: parts.rebuild,
        storage: parts.storage,
        production_recall: ProductionRecallReport {
            status: recall_status,
            correctness_status,
            performance_gate,
            public_service: parts.public_service,
            queries: parts.queries,
        },
        plan_validation,
        query_plans: parts.plans,
        required_matrix: required_matrix(),
    }
}

pub(crate) fn add_fts_limitations(plans: &mut Vec<BenchQueryPlan>, status: &UserMemoryIndexStatus) {
    for (lane, lane_status) in [
        ("fts_unicode", status.fts_unicode_status.as_str()),
        ("fts_trigram", status.fts_trigram_status.as_str()),
    ] {
        if !plans.iter().any(|plan| plan.lane == lane) {
            plans.push(unavailable_fts_plan(lane, lane_status));
        }
    }
}

fn unavailable_fts_plan(lane: &str, status: &str) -> BenchQueryPlan {
    BenchQueryPlan {
        lane: lane.to_string(),
        sql: String::new(),
        details: Vec::new(),
        error: Some(format!("production FTS lane status: {status}")),
        required_index: None,
        required_index_hit: None,
    }
}

fn corpus_report(config: &Config, jsonl: &str, path: &Path) -> CorpusReport {
    CorpusReport {
        size: config.size,
        seed: config.seed,
        digest: digest(jsonl),
        fixture_digests: fixture_digests(),
        output: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    }
}

fn methods() -> MethodsReport {
    MethodsReport {
        cold: "five fresh child processes per slice; OS page cache is not flushed",
        warm: "30 internal calls per slice plus 30 public service calls at 10k",
        network: "disabled by design",
        llm: "disabled by design",
        adapter:
            "production DB init, source refresh, timeout wrapper, query lanes, ranking, and hydrate",
        limitations: [
            "OS page cache is not flushed between fresh-process samples",
            "internal warm slices exclude source policy and checkpoint digest gates",
            "public end-to-end warm measurement covers the exact 10k slice only",
            "10k public source uses one user-memory entry and 9999 compact profile paragraphs",
            "synthetic benchmark injects contradiction relations; durable source ingestion is excluded",
        ],
    }
}

fn required_matrix() -> Matrix {
    Matrix {
        active_items: ["1k", "10k", "50k"],
        query_slices: [
            "exact",
            "cjk",
            "short",
            "fts_union",
            "temporal",
            "relation",
            "conflict",
            "abstain",
        ],
        cache_states: ["process_cold", "process_warm"],
        execution_modes: ["single_process", "fresh_child_process"],
    }
}

fn environment(status: &UserMemoryIndexStatus) -> EnvironmentReport {
    EnvironmentReport {
        git_commit: git_output(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unavailable".to_string()),
        git_dirty: git_dirty(),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu: std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown".to_string()),
        sqlite_fts_capability: format!(
            "unicode={},trigram={}",
            status.fts_unicode_status, status.fts_trigram_status
        ),
        connection_pool: "production init_database pool: max=5, WAL, synchronous=NORMAL",
    }
}

fn git_dirty() -> String {
    match git_output(&["status", "--porcelain"]) {
        Some(value) if value.is_empty() => "false".to_string(),
        Some(_) => "true".to_string(),
        None => "unavailable".to_string(),
    }
}

fn git_output(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn fixture_digests() -> BTreeMap<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/user_memory_recall/v1");
    ["calibration.jsonl", "holdout.jsonl"]
        .into_iter()
        .filter_map(|name| {
            fs::read(root.join(name))
                .ok()
                .map(|bytes| (name.to_string(), digest_bytes(&bytes)))
        })
        .collect()
}
