use chrono::{DateTime, SecondsFormat, Utc};

use super::helpers::contains_potential_secret;
use super::index_types::{IndexEvidence, IndexItem, IndexItemSource};
use super::{UserMemoryCandidate, UserMemoryLearningState};

const CANDIDATE_IMPORTANCE: f64 = 0.45;

pub(super) fn add_active_candidates(
    items: &mut Vec<IndexItem>,
    state: Option<&UserMemoryLearningState>,
) {
    let Some(state) = state else { return };
    let active = state
        .candidates
        .iter()
        .filter(|item| !item.status.is_terminal());
    for candidate in active {
        let mut item = IndexItem::new(
            candidate.id.clone(),
            candidate.content.clone(),
            IndexItemSource {
                kind: "candidate".into(),
                revision: candidate.last_observed_at.clone(),
            },
        );
        item.trust_class = "candidate".into();
        item.confidence = i64::from(candidate.confidence);
        item.importance = CANDIDATE_IMPORTANCE;
        item.sensitive = contains_potential_secret(&item.content);
        for wording in std::iter::once(&candidate.content).chain(&candidate.wording_variants) {
            if !contains_potential_secret(wording) {
                item.add_alias("candidate_wording", wording);
            }
        }
        add_observations(&mut item, candidate);
        items.push(item);
    }
}

fn add_observations(item: &mut IndexItem, candidate: &UserMemoryCandidate) {
    for observation in &candidate.observations {
        let Ok(observed) = DateTime::parse_from_rfc3339(&observation.observed_at) else {
            continue;
        };
        item.add_evidence(IndexEvidence {
            source_kind: "candidate_observation".into(),
            source_id: observation.opaque_source_id.clone(),
            conversation_id: observation.conversation_id.clone(),
            turn_nonce: observation.turn_nonce as i64,
            excerpt_digest: candidate.deduplication_digest.clone(),
            observed_at: observed
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Millis, true),
        });
    }
}
