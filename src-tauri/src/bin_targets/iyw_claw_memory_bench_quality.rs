use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use iyw_claw_lib::user_memory::bench::{BenchQuery, BenchRecallMeasurement, ProductionBench};

use super::iyw_claw_memory_bench_config::{output_dir, write_file};
use super::iyw_claw_memory_bench_fixture::{
    bench_inputs, load_split, ordered_outcome, validate_split, QualityFailure, QualityReport,
    QualitySplitReport, RecallFixture, TagQuality,
};

const DETERMINISTIC_ITERATIONS: usize = 3;
const QUALITY_REPORT_NAME: &str = "quality-recall-calibration-v1.json";

struct CaseResult {
    passed: bool,
    allow_hit: bool,
    forbidden_exposures: usize,
    abstention_correct: bool,
    deterministic: bool,
    exact_top1: Option<bool>,
    exact_lane: Option<bool>,
    alias_lane: Option<bool>,
    failure: Option<QualityFailure>,
}

pub(crate) async fn write_quality_report() -> Result<PathBuf, String> {
    std::fs::create_dir_all(output_dir()).map_err(|error| error.to_string())?;
    let report = evaluate_quality().await?;
    let path = output_dir().join(QUALITY_REPORT_NAME);
    let encoded = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    write_file(&path, &encoded)?;
    Ok(path)
}

async fn evaluate_quality() -> Result<QualityReport, String> {
    let mut splits = BTreeMap::new();
    for split in ["calibration", "holdout"] {
        splits.insert(split.to_string(), evaluate_split(split).await?);
    }
    let passed = splits.values().all(|split| split.failed == 0);
    Ok(QualityReport {
        schema_version: "MemoryRecallQualityReportV1",
        dataset_version: "RecallCalibrationV1",
        ranking_version: "rrf-v1",
        deterministic_iterations: DETERMINISTIC_ITERATIONS,
        status: if passed { "passed" } else { "failed" },
        limitations: vec![
            "host memory truth source does not yet emit explicit contradiction relations",
        ],
        splits,
    })
}

async fn evaluate_split(split: &str) -> Result<QualitySplitReport, String> {
    let fixtures = load_split(split)?;
    let first = fixtures
        .first()
        .ok_or_else(|| format!("{split} fixture split is empty"))?;
    validate_split(&fixtures)?;
    let temp = tempfile::Builder::new()
        .prefix(&format!("memory-quality-{split}-"))
        .tempdir_in(output_dir())
        .map_err(|error| error.to_string())?;
    let (bench, _) = ProductionBench::create(
        temp.path(),
        first.fixture_digest.clone(),
        bench_inputs(first),
    )
    .await?;
    let result = evaluate_cases(&bench, &fixtures).await;
    bench.close().await?;
    drop(temp);
    result
}

async fn evaluate_cases(
    bench: &ProductionBench,
    fixtures: &[RecallFixture],
) -> Result<QualitySplitReport, String> {
    let mut report = empty_split_report(fixtures.len());
    for (index, fixture) in fixtures.iter().enumerate() {
        if index > 0 {
            bench
                .replace(fixture.fixture_digest.clone(), bench_inputs(fixture))
                .await?;
        }
        let result = evaluate_case(bench, fixture).await?;
        record_case(&mut report, fixture, result);
    }
    report.failed = report.fixture_count.saturating_sub(report.passed);
    report.status = if report.failed == 0 {
        "passed"
    } else {
        "failed"
    };
    Ok(report)
}

async fn evaluate_case(
    bench: &ProductionBench,
    fixture: &RecallFixture,
) -> Result<CaseResult, String> {
    let query = BenchQuery {
        name: fixture.fixture_id.clone(),
        text: fixture.query.text.clone(),
        query_at: fixture.query.query_at.clone(),
        limit: 8,
        scope_type: fixture.query.scope.kind.clone(),
        scope_key: fixture.query.scope.key.clone(),
    };
    let mut samples = Vec::with_capacity(DETERMINISTIC_ITERATIONS);
    for _ in 0..DETERMINISTIC_ITERATIONS {
        samples.push(bench.recall(query.clone()).await?);
    }
    Ok(score_case(fixture, &samples))
}

fn score_case(fixture: &RecallFixture, samples: &[BenchRecallMeasurement]) -> CaseResult {
    let first = samples.first().expect("quality runner records samples");
    let returned = first
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    let allow_hit = fixture
        .expected
        .allow_ids
        .iter()
        .all(|id| returned.contains(id.as_str()));
    let forbidden = fixture
        .expected
        .forbid_ids
        .iter()
        .filter(|id| returned.contains(id.as_str()))
        .count();
    let abstention = first.abstained == fixture.expected.abstain;
    let deterministic = deterministic(samples);
    let exact_top1 = tagged_metric(fixture, "exact", exact_top1(fixture, first));
    let exact_lane = tagged_metric(fixture, "exact", lane_hit(fixture, first, "exact"));
    let alias_lane = tagged_metric(fixture, "alias", lane_hit(fixture, first, "alias"));
    let passed = allow_hit
        && forbidden == 0
        && abstention
        && deterministic
        && exact_top1.unwrap_or(true)
        && exact_lane.unwrap_or(true)
        && alias_lane.unwrap_or(true);
    let mut result = CaseResult {
        passed,
        allow_hit,
        forbidden_exposures: forbidden,
        abstention_correct: abstention,
        deterministic,
        exact_top1,
        exact_lane,
        alias_lane,
        failure: None,
    };
    if !passed {
        result.failure = Some(failure(fixture, first, &result));
    }
    result
}

fn tagged_metric(fixture: &RecallFixture, tag: &str, value: bool) -> Option<bool> {
    fixture
        .tags
        .iter()
        .any(|candidate| candidate == tag)
        .then_some(value)
}

fn exact_top1(fixture: &RecallFixture, sample: &BenchRecallMeasurement) -> bool {
    sample
        .items
        .first()
        .is_some_and(|item| fixture.expected.allow_ids.iter().any(|id| id == &item.id))
}

fn lane_hit(fixture: &RecallFixture, sample: &BenchRecallMeasurement, lane: &str) -> bool {
    sample.items.iter().any(|item| {
        fixture.expected.allow_ids.iter().any(|id| id == &item.id)
            && item.lanes.iter().any(|candidate| candidate == lane)
    })
}

fn deterministic(samples: &[BenchRecallMeasurement]) -> bool {
    let Some(first) = samples.first() else {
        return false;
    };
    samples.iter().all(|sample| {
        ordered_outcome(sample) == ordered_outcome(first)
            && sample.abstained == first.abstained
            && sample.reason_codes == first.reason_codes
    })
}

fn failure(
    fixture: &RecallFixture,
    sample: &BenchRecallMeasurement,
    result: &CaseResult,
) -> QualityFailure {
    let mut reasons = Vec::new();
    if !result.allow_hit {
        reasons.push("allow_id_missing");
    }
    if result.forbidden_exposures > 0 {
        reasons.push("forbidden_id_exposed");
    }
    if !result.abstention_correct {
        reasons.push("abstention_mismatch");
    }
    if !result.deterministic {
        reasons.push("nondeterministic_output");
    }
    if result.exact_lane == Some(false) {
        reasons.push("exact_lane_missing");
    }
    if result.alias_lane == Some(false) {
        reasons.push("alias_lane_missing");
    }
    QualityFailure {
        fixture_id: fixture.fixture_id.clone(),
        reasons,
        returned_ids: sample.items.iter().map(|item| item.id.clone()).collect(),
    }
}

fn empty_split_report(fixture_count: usize) -> QualitySplitReport {
    QualitySplitReport {
        status: "measured",
        fixture_count,
        passed: 0,
        failed: 0,
        allow_hit_count: 0,
        forbidden_exposure_count: 0,
        abstention_correct_count: 0,
        deterministic_count: 0,
        exact_case_count: 0,
        exact_top1_count: 0,
        exact_lane_count: 0,
        alias_case_count: 0,
        alias_lane_count: 0,
        per_tag: BTreeMap::new(),
        failures: Vec::new(),
    }
}

fn record_case(report: &mut QualitySplitReport, fixture: &RecallFixture, result: CaseResult) {
    report.passed += usize::from(result.passed);
    report.allow_hit_count += usize::from(result.allow_hit);
    report.forbidden_exposure_count += result.forbidden_exposures;
    report.abstention_correct_count += usize::from(result.abstention_correct);
    report.deterministic_count += usize::from(result.deterministic);
    if let Some(value) = result.exact_top1 {
        report.exact_case_count += 1;
        report.exact_top1_count += usize::from(value);
    }
    report.exact_lane_count += usize::from(result.exact_lane.unwrap_or(false));
    if let Some(value) = result.alias_lane {
        report.alias_case_count += 1;
        report.alias_lane_count += usize::from(value);
    }
    for tag in &fixture.tags {
        let metric = report
            .per_tag
            .entry(tag.clone())
            .or_insert_with(TagQuality::default);
        metric.count += 1;
        metric.passed += usize::from(result.passed);
    }
    if let Some(failure) = result.failure {
        report.failures.push(failure);
    }
}
