use std::collections::HashMap;

use chrono::{DateTime, SecondsFormat, Utc};

use super::helpers::{hash_parts, memory_entry_id};
use super::index_types::{
    normalize_alias, IndexAlias, IndexEvidence, IndexItem, IndexItemSource, IndexSnapshot,
};
use super::recall_types::USER_MEMORY_MAX_RECALL_QUERY_CHARS;
use super::{
    UserMemoryCandidate, UserMemoryCandidateStatus, UserMemoryDocumentId, UserMemoryLearningState,
    UserMemorySettingsSnapshot,
};

const SOURCE_KEY: &str = "user_memory";

struct DocumentSource<'a> {
    id: UserMemoryDocumentId,
    content: &'a str,
    file_name: &'a str,
    revision: &'a str,
}

pub(super) fn build_index_snapshot(
    settings: &UserMemorySettingsSnapshot,
    candidates: Option<&UserMemoryLearningState>,
) -> IndexSnapshot {
    let source_digest = source_digest(settings, candidates);
    let mut items = Vec::new();
    let mut item_positions = HashMap::new();

    if !settings.enabled {
        return IndexSnapshot {
            source_key: SOURCE_KEY.to_string(),
            source_digest,
            items,
            relations: Vec::new(),
        };
    }

    for id in UserMemoryDocumentId::ALL {
        let Some(document) = settings.documents.get(&id) else {
            continue;
        };
        if !document.enabled || !document.readable || document.content.trim().is_empty() {
            continue;
        }
        let source = DocumentSource {
            id,
            content: document.content.as_str(),
            file_name: &document.file_name,
            revision: &settings.revision,
        };
        match id {
            UserMemoryDocumentId::Memory => {
                add_memory_entries(&mut items, &mut item_positions, &source)
            }
            UserMemoryDocumentId::Profile | UserMemoryDocumentId::Soul => {
                add_markdown_paragraphs(&mut items, &mut item_positions, &source)
            }
        }
    }

    add_candidate_evidence(&mut items, candidates);
    IndexSnapshot {
        source_key: SOURCE_KEY.to_string(),
        source_digest,
        items,
        relations: Vec::new(),
    }
}

pub(super) fn source_digest(
    settings: &UserMemorySettingsSnapshot,
    candidates: Option<&UserMemoryLearningState>,
) -> String {
    let candidate_bytes = candidates
        .and_then(|state| serde_json::to_vec(state).ok())
        .unwrap_or_default();
    hash_parts(&[settings.revision.as_bytes(), &candidate_bytes])
}

fn add_memory_entries(
    items: &mut Vec<IndexItem>,
    item_positions: &mut HashMap<(String, String), usize>,
    source: &DocumentSource<'_>,
) {
    for line in source.content.lines() {
        let Some((entry_id, value, observed_at)) = parse_memory_line(line) else {
            continue;
        };
        let mut item = IndexItem::new(
            entry_id,
            value,
            IndexItemSource {
                kind: "memory".to_string(),
                revision: source.revision.to_string(),
            },
        );
        item.importance = 0.7;
        item.sensitive = super::helpers::contains_potential_secret(&item.content);
        item.add_alias("document", source.file_name);
        item.add_evidence(document_evidence(
            source.file_name,
            &item.id,
            observed_at.as_deref().unwrap_or_else(|| ""),
        ));
        push_index_item(items, item_positions, item);
    }
}

fn add_markdown_paragraphs(
    items: &mut Vec<IndexItem>,
    item_positions: &mut HashMap<(String, String), usize>,
    source: &DocumentSource<'_>,
) {
    for paragraph in source.content.split("\n\n") {
        let value = paragraph.trim();
        if value.is_empty() {
            continue;
        }
        let digest = hash_parts(&[source.id.file_name().as_bytes(), value.as_bytes()]);
        let item_id = format!("iyw-{}-{}", document_kind(source.id), &digest[..20]);
        let mut item = IndexItem::new(
            item_id,
            value.to_string(),
            IndexItemSource {
                kind: document_kind(source.id).to_string(),
                revision: source.revision.to_string(),
            },
        );
        item.importance = 1.0;
        item.sensitive = super::helpers::contains_potential_secret(&item.content);
        item.add_alias("document", source.file_name);
        item.add_evidence(document_evidence(source.file_name, &item.id, ""));
        push_index_item(items, item_positions, item);
    }
}

fn push_index_item(
    items: &mut Vec<IndexItem>,
    item_positions: &mut HashMap<(String, String), usize>,
    item: IndexItem,
) {
    let identity = (item.id.clone(), item.content_digest.clone());
    let Some(&position) = item_positions.get(&identity) else {
        item_positions.insert(identity, items.len());
        items.push(item);
        return;
    };
    let existing = &mut items[position];
    existing.sensitive |= item.sensitive;
    existing.importance = existing.importance.max(item.importance);
    for alias in item.aliases {
        existing.add_alias(&alias.kind, alias.value);
    }
    for evidence in item.evidence {
        existing.add_evidence(evidence);
    }
}

fn add_candidate_evidence(items: &mut [IndexItem], state: Option<&UserMemoryLearningState>) {
    let Some(state) = state else {
        return;
    };
    let mut item_positions = HashMap::with_capacity(items.len());
    for (position, item) in items.iter().enumerate() {
        item_positions.entry(item.id.clone()).or_insert(position);
    }
    for candidate in state
        .candidates
        .iter()
        .filter(|candidate| candidate.status == UserMemoryCandidateStatus::Confirmed)
    {
        let Some(entry_id) = candidate.confirmed_memory_entry_id.as_deref() else {
            continue;
        };
        let Some(&position) = item_positions.get(entry_id) else {
            continue;
        };
        let item = &mut items[position];
        add_confirmed_wording_aliases(item, candidate);
        for observation in &candidate.observations {
            let Some(observed_at) = canonical_observed_at(&observation.observed_at) else {
                continue;
            };
            item.add_evidence(IndexEvidence {
                source_kind: "candidate_observation".to_string(),
                source_id: observation.opaque_source_id.clone(),
                conversation_id: None,
                turn_nonce: observation.turn_nonce as i64,
                excerpt_digest: candidate.deduplication_digest.clone(),
                observed_at,
            });
        }
    }
}

fn add_confirmed_wording_aliases(item: &mut IndexItem, candidate: &UserMemoryCandidate) {
    let confirmed_without_edit = candidate.resolved_content.as_deref()
        == Some(candidate.content.as_str())
        && item.content == candidate.content;
    if !confirmed_without_edit {
        return;
    }
    for wording in std::iter::once(&candidate.content).chain(&candidate.wording_variants) {
        if wording.chars().count() > USER_MEMORY_MAX_RECALL_QUERY_CHARS
            || super::helpers::contains_potential_secret(wording)
        {
            continue;
        }
        item.add_alias("confirmed_wording", wording.clone());
    }
}

fn parse_memory_line(line: &str) -> Option<(String, String, Option<String>)> {
    let marker_start = line.find("<!-- iyw-memory-")?;
    let marker_end = line[marker_start..].find(" -->")? + marker_start;
    let entry_id = line[marker_start + 5..marker_end].trim().to_string();
    let raw = line[..marker_start].trim();
    let value = strip_memory_prefix(raw);
    if value.is_empty() {
        return None;
    }
    let observed_at = first_bracket_value(raw).and_then(|value| canonical_observed_at(&value));
    let entry_id = if entry_id.starts_with("iyw-memory-") {
        entry_id
    } else {
        memory_entry_id(&value)
    };
    Some((entry_id, value, observed_at))
}

fn strip_memory_prefix(value: &str) -> String {
    let mut value = value.trim_start_matches("- ").trim();
    for _ in 0..2 {
        if !value.starts_with('[') {
            break;
        }
        let Some(end) = value.find("] ") else {
            break;
        };
        value = value[end + 2..].trim();
    }
    value.trim().to_string()
}

fn first_bracket_value(value: &str) -> Option<String> {
    let value = value.trim_start_matches("- ").trim();
    let end = value.strip_prefix('[')?.find(']')? + 1;
    Some(value[1..end].to_string())
}

fn canonical_observed_at(value: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(value).ok().map(|value| {
        value
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
    })
}

fn document_evidence(file_name: &str, item_id: &str, observed_at: &str) -> IndexEvidence {
    let observed_at = if observed_at.is_empty() {
        "unknown".to_string()
    } else {
        observed_at.to_string()
    };
    IndexEvidence {
        source_kind: "user_memory_document".to_string(),
        source_id: format!("{file_name}#{item_id}"),
        conversation_id: None,
        turn_nonce: 0,
        excerpt_digest: hash_parts(&[item_id.as_bytes(), observed_at.as_bytes()]),
        observed_at,
    }
}

fn document_kind(id: UserMemoryDocumentId) -> &'static str {
    match id {
        UserMemoryDocumentId::Memory => "memory",
        UserMemoryDocumentId::Profile => "profile",
        UserMemoryDocumentId::Soul => "soul",
    }
}

pub(super) fn alias_row(alias: &IndexAlias) -> (String, String) {
    (alias.kind.clone(), normalize_alias(&alias.value))
}
