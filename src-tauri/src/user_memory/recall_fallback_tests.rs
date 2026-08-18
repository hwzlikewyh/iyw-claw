use chrono::{DateTime, Utc};

use super::index_types::{IndexEvidence, IndexItem, IndexItemSource, IndexSnapshot};
use super::recall_fallback_scan::{scan_snapshot, FallbackScanContext};

const QUERY_AT: &str = "2026-08-17T12:00:00Z";

fn item(id: &str, content: &str) -> IndexItem {
    IndexItem::new(
        id.to_string(),
        content.to_string(),
        IndexItemSource {
            kind: "memory".to_string(),
            revision: "revision-1".to_string(),
        },
    )
}

fn snapshot(items: Vec<IndexItem>) -> IndexSnapshot {
    IndexSnapshot {
        source_key: "user_memory".to_string(),
        source_digest: "fixture-digest".to_string(),
        items,
        relations: Vec::new(),
    }
}

fn scan(snapshot: &IndexSnapshot, query: &str) -> super::recall_fallback_scan::FallbackScan {
    let query_at = DateTime::parse_from_rfc3339(QUERY_AT)
        .unwrap()
        .with_timezone(&Utc);
    scan_snapshot(FallbackScanContext {
        snapshot,
        query,
        limit: 8,
        query_at: Some(&query_at),
        scope: &super::UserMemoryRecallScope::global(),
    })
}

#[test]
fn stable_id_exact_match_is_preserved() {
    let snapshot = snapshot(vec![item("iyw-memory-exact-1", "unrelated text")]);

    let result = scan(&snapshot, "iyw-memory-exact-1");

    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].id, "iyw-memory-exact-1");
    assert!(result.items[0]
        .lanes
        .iter()
        .any(|lane| lane == "source_exact"));
}

#[test]
fn forbidden_items_are_never_returned() {
    let mut sensitive = item("iyw-memory-sensitive", "maple status");
    sensitive.sensitive = true;
    let mut workspace = item("iyw-memory-workspace", "maple status");
    workspace.scope_type = "workspace".to_string();
    workspace.scope_key = "workspace-other".to_string();
    let mut derived = item("iyw-memory-derived", "maple status");
    derived.trust_class = "host_derived".to_string();

    let result = scan(
        &snapshot(vec![sensitive, workspace, derived]),
        "maple status",
    );

    assert!(result.items.is_empty());
}

#[test]
fn no_answer_query_abstains_instead_of_filling_the_limit() {
    let snapshot = snapshot(vec![item(
        "iyw-memory-public",
        "The public release checklist contains four steps.",
    )]);

    let result = scan(&snapshot, "zephyr credential");

    assert!(result.items.is_empty());
    assert_eq!(result.union_count, 0);
}

#[test]
fn tied_results_use_canonical_id_order_for_one_hundred_runs() {
    let snapshot = snapshot(vec![
        item("iyw-memory-b", "shared phrase"),
        item("iyw-memory-a", "shared phrase"),
    ]);
    let expected = serde_json::to_vec(&scan(&snapshot, "shared phrase").items).unwrap();

    for _ in 0..100 {
        let actual = serde_json::to_vec(&scan(&snapshot, "shared phrase").items).unwrap();
        assert_eq!(actual, expected);
    }
    let result = scan(&snapshot, "shared phrase");
    assert_eq!(result.items[0].id, "iyw-memory-a");
    assert_eq!(result.items[1].id, "iyw-memory-b");
}

#[test]
fn temporal_lane_returns_only_matching_current_evidence() {
    let mut matching = item("iyw-memory-temporal", "unrelated schedule");
    matching.add_evidence(IndexEvidence {
        source_kind: "conversation".to_string(),
        source_id: "source-1".to_string(),
        conversation_id: Some("conversation-1".to_string()),
        turn_nonce: 1,
        excerpt_digest: "evidence-1".to_string(),
        observed_at: "2026-07-09T09:30:00Z".to_string(),
    });
    let mut expired = item("iyw-memory-expired", "unrelated schedule");
    expired.valid_to = Some("2026-01-01T00:00:00Z".to_string());
    expired.evidence = matching.evidence.clone();

    let result = scan(
        &snapshot(vec![matching, expired]),
        "what changed on 2026-07-09?",
    );

    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].id, "iyw-memory-temporal");
    assert!(result.items[0]
        .lanes
        .iter()
        .any(|lane| lane == "source_temporal"));
}
