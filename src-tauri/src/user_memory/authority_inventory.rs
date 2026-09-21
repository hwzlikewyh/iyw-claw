use super::authority_records::RecordPayload;
use super::authority_types::AuthorityData;
use super::index_types::{IndexEvidence, IndexItem, IndexItemSource, IndexRelation};
use super::{ResourceGeneration, UserMemoryLearningState, UserMemoryService};
use crate::app_error::AppCommandError;
use std::collections::BTreeMap;

pub(super) fn records(
    service: &UserMemoryService,
    data: &AuthorityData,
) -> Result<Vec<RecordPayload>, AppCommandError> {
    let snapshot = service.authority_index_snapshot(data)?;
    super::index_integrity::validate_snapshot_identities(&snapshot)
        .map_err(super::index_checkpoint::database_error)?;
    let mut records = snapshot
        .items
        .into_iter()
        .map(|item| {
            let relations = snapshot
                .relations
                .iter()
                .filter(|relation| relation.source_id == item.id)
                .cloned()
                .collect();
            (
                item.id.clone(),
                RecordPayload {
                    item,
                    state: "active".into(),
                    relations,
                    source: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    if let ResourceGeneration::Present { value, .. } = &data.learning {
        add_candidates(&mut records, value)?;
        add_experiences(&mut records, value)?;
        add_views(&mut records, value)?;
        for (id, record) in &mut records {
            if let Some(retention) = value.retention.get(id) {
                record.source =
                    Some(serde_json::json!({"record": record.source, "retention": retention}));
            }
        }
    }
    for record in records.values_mut() {
        record.item.source_revision.clear();
        record.item.sensitive |= super::helpers::contains_potential_secret(&record.item.content);
        if record.item.valid_to.is_some() && record.state == "active" {
            record.state = "validity_limited".into();
        }
    }
    Ok(records.into_values().collect())
}

fn missing_item(id: &str, content: &str, kind: &str) -> RecordPayload {
    RecordPayload {
        item: IndexItem::new(
            id.into(),
            content.into(),
            IndexItemSource {
                kind: kind.into(),
                revision: String::new(),
            },
        ),
        state: "active".into(),
        relations: Vec::new(),
        source: None,
    }
}

fn add_candidates(
    records: &mut BTreeMap<String, RecordPayload>,
    state: &UserMemoryLearningState,
) -> Result<(), AppCommandError> {
    for candidate in &state.candidates {
        let record = records
            .entry(candidate.id.clone())
            .or_insert_with(|| missing_item(&candidate.id, &candidate.content, "candidate"));
        record.state = serde_json::to_value(candidate.status)
            .map_err(serialization_error)?
            .as_str()
            .unwrap_or("unknown")
            .into();
        record.source = Some(serde_json::to_value(candidate).map_err(serialization_error)?);
        record.item.trust_class = "candidate".into();
        record.item.confidence = candidate.confidence.into();
        for observation in &candidate.observations {
            record.item.add_evidence(IndexEvidence {
                source_kind: "candidate_observation".into(),
                source_id: observation.opaque_source_id.clone(),
                conversation_id: observation.conversation_id.clone(),
                turn_nonce: observation.turn_nonce as i64,
                excerpt_digest: candidate.deduplication_digest.clone(),
                observed_at: observation.observed_at.clone(),
            });
        }
    }
    Ok(())
}

fn add_experiences(
    records: &mut BTreeMap<String, RecordPayload>,
    state: &UserMemoryLearningState,
) -> Result<(), AppCommandError> {
    for experience in &state.experiences {
        let record = records.entry(experience.id.clone()).or_insert_with(|| {
            let mut record = missing_item(&experience.id, &experience.content, "experience");
            record.state = if experience.superseded_by.is_some() {
                "superseded"
            } else {
                "unindexed"
            }
            .into();
            record
        });
        record.source = Some(serde_json::to_value(experience).map_err(serialization_error)?);
        record.item.trust_class = "agent_experience".into();
        record.item.confidence = experience.confidence.into();
        record.item.scope_type = experience.scope_type.clone();
        record.item.scope_key = experience.scope_key.clone();
        for evidence in &experience.evidence {
            record.item.add_evidence(IndexEvidence {
                source_kind: "agent_experience".into(),
                source_id: evidence.opaque_source_id.clone(),
                conversation_id: None,
                turn_nonce: evidence.turn_nonce as i64,
                excerpt_digest: experience.content_digest.clone(),
                observed_at: evidence.observed_at.clone(),
            });
        }
    }
    Ok(())
}

fn add_views(
    records: &mut BTreeMap<String, RecordPayload>,
    state: &UserMemoryLearningState,
) -> Result<(), AppCommandError> {
    for view in &state.generated_views {
        let id = super::retention_view::document_entry_id(view.document, &view.content);
        if records
            .get(&id)
            .is_some_and(|record| record.item.trust_class == "host_confirmed")
        {
            continue;
        }
        let valid = super::generated_overrides::permits(state, view)
            && view.sources.iter().all(|source| {
                records
                    .get(&source.id)
                    .is_some_and(|record| record.item.content_digest == source.digest)
            });
        let mut record = missing_item(&id, &view.content, "generated_view");
        record.state = if valid {
            "active"
        } else {
            "source_invalid_or_overridden"
        }
        .into();
        record.item.trust_class = "candidate".into();
        record.item.confidence = VIEW_CONFIDENCE;
        record.item.valid_to = view
            .sources
            .iter()
            .filter_map(|source| {
                state
                    .retention
                    .get(&source.id)
                    .map(|value| value.expires_at.clone())
            })
            .min();
        record.source = Some(serde_json::to_value(view).map_err(serialization_error)?);
        add_view_evidence(&mut record, view);
        records.insert(id, record);
    }
    Ok(())
}

const VIEW_CONFIDENCE: i64 = 70;

fn add_view_evidence(record: &mut RecordPayload, view: &super::GeneratedMemoryView) {
    for source in &view.sources {
        record.item.add_evidence(IndexEvidence {
            source_kind: "generated_view".into(),
            source_id: source.id.clone(),
            conversation_id: None,
            turn_nonce: 0,
            excerpt_digest: source.digest.clone(),
            observed_at: "unknown".into(),
        });
        record.relations.push(IndexRelation {
            source_id: record.item.id.clone(),
            relation: "derived_from".into(),
            target_id: source.id.clone(),
            confidence: VIEW_CONFIDENCE,
            created_at: "unknown".into(),
        });
    }
}

fn serialization_error(error: serde_json::Error) -> AppCommandError {
    AppCommandError::configuration_invalid("Cannot encode memory source evidence")
        .with_detail(error.to_string())
}
