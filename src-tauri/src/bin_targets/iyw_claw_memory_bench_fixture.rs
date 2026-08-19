use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use iyw_claw_lib::user_memory::bench::{BenchMemoryInput, BenchRecallMeasurement};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub(crate) struct RecallFixture {
    pub dataset_version: String,
    pub fixture_id: String,
    pub memory: Vec<FixtureMemory>,
    pub query: FixtureQuery,
    pub expected: FixtureExpected,
    pub tags: Vec<String>,
    #[serde(skip_serializing)]
    pub fixture_digest: String,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct FixtureMemory {
    pub id: String,
    pub content: String,
    pub scope: FixtureScope,
    pub sensitive: bool,
    pub superseded_by: Option<String>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<FixtureRelation>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct FixtureRelation {
    #[serde(rename = "type")]
    pub kind: String,
    pub target_id: String,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct FixtureQuery {
    pub text: String,
    pub query_at: String,
    pub scope: FixtureScope,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct FixtureExpected {
    pub allow_ids: Vec<String>,
    pub forbid_ids: Vec<String>,
    pub abstain: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict_policy: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct FixtureScope {
    #[serde(rename = "type")]
    pub kind: String,
    pub key: String,
}

#[derive(Serialize)]
pub(crate) struct QualityReport {
    pub schema_version: &'static str,
    pub dataset_version: &'static str,
    pub ranking_version: &'static str,
    pub deterministic_iterations: usize,
    pub status: &'static str,
    pub limitations: Vec<&'static str>,
    pub splits: BTreeMap<String, QualitySplitReport>,
}

#[derive(Serialize)]
pub(crate) struct QualitySplitReport {
    pub status: &'static str,
    pub fixture_count: usize,
    pub passed: usize,
    pub failed: usize,
    pub allow_hit_count: usize,
    pub forbidden_exposure_count: usize,
    pub abstention_correct_count: usize,
    pub deterministic_count: usize,
    pub exact_case_count: usize,
    pub exact_top1_count: usize,
    pub exact_lane_count: usize,
    pub alias_case_count: usize,
    pub alias_lane_count: usize,
    pub per_tag: BTreeMap<String, TagQuality>,
    pub failures: Vec<QualityFailure>,
}

#[derive(Default, Serialize)]
pub(crate) struct TagQuality {
    pub count: usize,
    pub passed: usize,
}

#[derive(Serialize)]
pub(crate) struct QualityFailure {
    pub fixture_id: String,
    pub reasons: Vec<&'static str>,
    pub returned_ids: Vec<String>,
}

pub(crate) fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/user_memory_recall/v1")
}

pub(crate) fn load_split(split: &str) -> Result<Vec<RecallFixture>, String> {
    let path = fixture_root().join(format!("{split}.jsonl"));
    let raw = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    raw.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line)
                .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))
        })
        .collect()
}

pub(crate) fn validate_split(fixtures: &[RecallFixture]) -> Result<(), String> {
    let ids = fixtures
        .iter()
        .map(|fixture| fixture.fixture_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if ids.len() != fixtures.len() {
        return Err("fixture IDs are not unique".to_string());
    }
    if fixtures
        .iter()
        .any(|fixture| fixture.dataset_version != "RecallCalibrationV1")
    {
        return Err("unexpected fixture dataset version".to_string());
    }
    for fixture in fixtures {
        if !fixture_digest_matches(fixture)? {
            return Err(format!("fixture digest mismatch: {}", fixture.fixture_id));
        }
    }
    Ok(())
}

pub(crate) fn bench_inputs(fixture: &RecallFixture) -> Vec<BenchMemoryInput> {
    fixture
        .memory
        .iter()
        .map(|memory| BenchMemoryInput {
            id: memory.id.clone(),
            kind: "memory".to_string(),
            content: memory.content.clone(),
            content_digest: digest(memory.content.as_bytes()),
            aliases: memory.aliases.clone(),
            scope_type: memory.scope.kind.clone(),
            scope_key: memory.scope.key.clone(),
            sensitive: memory.sensitive,
            superseded_by: memory.superseded_by.clone(),
            source_revision: fixture.fixture_id.clone(),
            valid_from: memory.valid_from.clone(),
            valid_to: memory.valid_to.clone(),
            relation_ids: relation_targets(memory, "related"),
            contradicts_ids: relation_targets(memory, "contradicts"),
        })
        .collect()
}

fn relation_targets(memory: &FixtureMemory, relation: &str) -> Vec<String> {
    memory
        .relations
        .iter()
        .filter(|candidate| candidate.kind == relation)
        .map(|candidate| candidate.target_id.clone())
        .collect()
}

pub(crate) fn ordered_outcome(sample: &BenchRecallMeasurement) -> Vec<(&str, &[String])> {
    sample
        .items
        .iter()
        .map(|item| (item.id.as_str(), item.lanes.as_slice()))
        .collect()
}

pub(crate) fn fixture_digest_matches(fixture: &RecallFixture) -> Result<bool, String> {
    let encoded = serde_json::to_vec(fixture).map_err(|error| error.to_string())?;
    Ok(digest(&encoded) == fixture.fixture_digest)
}

fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    format!("sha256:{:x}", Sha256::digest(bytes))
}
