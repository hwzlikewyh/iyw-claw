use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};

use super::index_types::{normalize_alias, IndexItem, IndexSnapshot};
use super::recall_scope::UserMemoryRecallScope;
use super::recall_temporal::{temporal_key, MAX_TEMPORAL_CANDIDATES};
use super::recall_types::{
    bounded_recall_content, UserMemoryRecallItem, MAX_RECALL_ITEM_CHARS, MAX_RECALL_TOTAL_CHARS,
};
use super::recall_validity::item_is_current_at;

const MAX_WEAK_FALLBACK_ITEMS: usize = 3;

struct FallbackHit<'a> {
    item: &'a IndexItem,
    score: f64,
    lanes: BTreeSet<String>,
    strong: bool,
    temporal_observed_at: Option<DateTime<Utc>>,
}

pub(super) struct FallbackScan {
    pub items: Vec<UserMemoryRecallItem>,
    pub conflicting_item_count: usize,
    pub unresolved_conflict_count: usize,
    pub union_count: usize,
    pub exact_count: usize,
    pub alias_count: usize,
    pub lexical_count: usize,
    pub temporal_count: usize,
}

pub(super) struct FallbackScanContext<'a> {
    pub snapshot: &'a IndexSnapshot,
    pub query: &'a str,
    pub limit: usize,
    pub query_at: Option<&'a DateTime<Utc>>,
    pub scope: &'a UserMemoryRecallScope,
}

struct FallbackMatchContext<'a> {
    normalized_query: &'a str,
    allow_lexical: bool,
    temporal_hits: &'a BTreeMap<String, DateTime<Utc>>,
    query_at: Option<&'a DateTime<Utc>>,
    scope: &'a UserMemoryRecallScope,
}

pub(super) fn scan_snapshot(context: FallbackScanContext<'_>) -> FallbackScan {
    let normalized = normalize_alias(context.query);
    let temporal_key = temporal_key(context.query);
    let temporal_hits = temporal_key
        .as_deref()
        .map(|key| newest_temporal_hits(context.snapshot, key, context.query_at, context.scope))
        .unwrap_or_default();
    let conflicting_ids = conflicting_item_ids(context.snapshot);
    let unresolved_ids = unresolved_conflict_ids(context.snapshot, context.query_at, context.scope);
    let mut seen_ids = BTreeSet::new();
    let match_context = FallbackMatchContext {
        normalized_query: &normalized,
        allow_lexical: context.query.chars().count() >= 3,
        temporal_hits: &temporal_hits,
        query_at: context.query_at,
        scope: context.scope,
    };
    let mut hits = context
        .snapshot
        .items
        .iter()
        .filter(|item| {
            !conflicting_ids.contains(item.id.as_str()) && seen_ids.insert(item.id.as_str())
        })
        .filter_map(|item| classify_item(item, &match_context))
        .collect::<Vec<_>>();
    let unresolved_conflict_count = hits
        .iter()
        .filter(|hit| unresolved_ids.contains(hit.item.id.as_str()))
        .count();
    hits.retain(|hit| !unresolved_ids.contains(hit.item.id.as_str()));
    let counts = fallback_lane_counts(&hits);
    hits.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| right.temporal_observed_at.cmp(&left.temporal_observed_at))
            .then_with(|| left.item.id.cmp(&right.item.id))
    });
    FallbackScan {
        union_count: hits.len(),
        items: budget_hits(hits, context.limit),
        conflicting_item_count: conflicting_ids.len(),
        unresolved_conflict_count,
        exact_count: counts[0],
        alias_count: counts[1],
        lexical_count: counts[2],
        temporal_count: counts[3],
    }
}

fn unresolved_conflict_ids<'a>(
    snapshot: &'a IndexSnapshot,
    query_at: Option<&DateTime<Utc>>,
    scope: &UserMemoryRecallScope,
) -> BTreeSet<&'a str> {
    let eligible = snapshot
        .items
        .iter()
        .filter(|item| eligible_item(item, query_at, scope))
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut conflicts = BTreeSet::new();
    for relation in snapshot
        .relations
        .iter()
        .filter(|relation| relation.relation == "contradicts")
    {
        if eligible.contains_key(relation.source_id.as_str())
            && eligible.contains_key(relation.target_id.as_str())
        {
            conflicts.insert(relation.source_id.as_str());
            conflicts.insert(relation.target_id.as_str());
        }
    }
    conflicts
}

fn conflicting_item_ids(snapshot: &IndexSnapshot) -> BTreeSet<&str> {
    let mut digests = BTreeMap::new();
    let mut conflicts = BTreeSet::new();
    for item in &snapshot.items {
        if let Some(existing) = digests.insert(item.id.as_str(), item.content_digest.as_str()) {
            if existing != item.content_digest.as_str() {
                conflicts.insert(item.id.as_str());
            }
        }
    }
    conflicts
}

fn newest_temporal_hits(
    snapshot: &IndexSnapshot,
    key: &str,
    query_at: Option<&DateTime<Utc>>,
    scope: &UserMemoryRecallScope,
) -> BTreeMap<String, DateTime<Utc>> {
    let mut hits = snapshot
        .items
        .iter()
        .filter(|item| eligible_item(item, query_at, scope))
        .filter_map(|item| newest_matching_evidence(item, key).map(|at| (item.id.clone(), at)))
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    hits.truncate(MAX_TEMPORAL_CANDIDATES);
    hits.into_iter().collect()
}

fn newest_matching_evidence(item: &IndexItem, key: &str) -> Option<DateTime<Utc>> {
    item.evidence
        .iter()
        .filter(|evidence| evidence.observed_at.contains(key))
        .filter_map(|evidence| DateTime::parse_from_rfc3339(&evidence.observed_at).ok())
        .map(|value| value.with_timezone(&Utc))
        .max()
}

fn classify_item<'a>(
    item: &'a IndexItem,
    context: &FallbackMatchContext<'_>,
) -> Option<FallbackHit<'a>> {
    if !eligible_item(item, context.query_at, context.scope) {
        return None;
    }
    let exact = item.id == context.normalized_query;
    let alias = item
        .aliases
        .iter()
        .any(|alias| normalize_alias(&alias.value) == context.normalized_query);
    let lexical =
        context.allow_lexical && normalize_alias(&item.content).contains(context.normalized_query);
    let temporal_observed_at = context.temporal_hits.get(&item.id).cloned();
    build_fallback_hit(
        item,
        [exact, alias, lexical, temporal_observed_at.is_some()],
        temporal_observed_at,
    )
}

fn eligible_item(
    item: &IndexItem,
    query_at: Option<&DateTime<Utc>>,
    scope: &UserMemoryRecallScope,
) -> bool {
    scope.permits(&item.scope_type, &item.scope_key)
        && item.trust_class == "host_confirmed"
        && !item.sensitive
        && query_at.is_some_and(|value| item_is_current_at(item, value))
}

fn build_fallback_hit(
    item: &IndexItem,
    matches: [bool; 4],
    temporal_observed_at: Option<DateTime<Utc>>,
) -> Option<FallbackHit<'_>> {
    if !matches.iter().any(|value| *value) {
        return None;
    }
    let mut lanes = BTreeSet::new();
    let mut score = 0.0;
    for (matched, lane, weight) in [
        (matches[0], "source_exact", 1.0),
        (matches[1], "source_alias", 0.9),
        (matches[2], "source_lexical", 0.6),
        (matches[3], "source_temporal", 0.5),
    ] {
        if matched {
            lanes.insert(lane.to_string());
            score += weight;
        }
    }
    Some(FallbackHit {
        item,
        score,
        lanes,
        strong: matches[0] || matches[1] || matches[3],
        temporal_observed_at,
    })
}

fn fallback_lane_counts(hits: &[FallbackHit<'_>]) -> [usize; 4] {
    let count = |lane: &str| hits.iter().filter(|hit| hit.lanes.contains(lane)).count();
    [
        count("source_exact"),
        count("source_alias"),
        count("source_lexical"),
        count("source_temporal"),
    ]
}

fn budget_hits(hits: Vec<FallbackHit<'_>>, limit: usize) -> Vec<UserMemoryRecallItem> {
    let mut weak_count = 0;
    let mut remaining = MAX_RECALL_TOTAL_CHARS;
    let mut items = Vec::new();
    for hit in hits {
        if !hit.strong && weak_count >= MAX_WEAK_FALLBACK_ITEMS {
            continue;
        }
        let Some(content) =
            bounded_recall_content(&hit.item.content, remaining.min(MAX_RECALL_ITEM_CHARS))
        else {
            continue;
        };
        remaining = remaining.saturating_sub(content.chars().count());
        if !hit.strong {
            weak_count += 1;
        }
        items.push(UserMemoryRecallItem {
            id: hit.item.id.clone(),
            kind: hit.item.kind.clone(),
            content,
            confidence: hit.item.confidence,
            importance: hit.item.importance,
            source_revision: hit.item.source_revision.clone(),
            score: hit.score,
            lanes: hit.lanes.into_iter().collect(),
        });
        if items.len() == limit {
            break;
        }
    }
    items
}
