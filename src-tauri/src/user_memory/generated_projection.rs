use super::generated_views::GeneratedMemoryView;
use super::index_types::{IndexEvidence, IndexItem, IndexItemSource, IndexRelation};
use super::{UserMemoryDocumentId, UserMemoryLearningState, UserMemorySettingsSnapshot};

const VIEW_CONFIDENCE: i64 = 70;

pub(super) fn append_generated(
    items: &mut Vec<IndexItem>,
    state: Option<&UserMemoryLearningState>,
    settings: &UserMemorySettingsSnapshot,
) -> Vec<IndexRelation> {
    let Some(state) = state else {
        return Vec::new();
    };
    let mut relations = Vec::new();
    for view in &state.generated_views {
        if !settings
            .documents
            .get(&view.document)
            .is_some_and(|document| document.enabled && document.readable)
            || !sources_current(view, items)
            || !super::generated_overrides::permits(state, view)
        {
            continue;
        }
        let mut item = view_item(view, items);
        if items.iter().any(|existing| existing.id == item.id) {
            continue;
        }
        for source in &view.sources {
            item.add_evidence(IndexEvidence {
                source_kind: "generated_view".into(),
                source_id: source.id.clone(),
                conversation_id: None,
                turn_nonce: 0,
                excerpt_digest: source.digest.clone(),
                observed_at: "unknown".into(),
            });
            relations.push(IndexRelation {
                source_id: item.id.clone(),
                relation: "derived_from".into(),
                target_id: source.id.clone(),
                confidence: VIEW_CONFIDENCE,
                created_at: "unknown".into(),
            });
        }
        items.push(item);
    }
    super::retention::apply_retention(items, Some(state));
    relations
}

fn sources_current(view: &GeneratedMemoryView, items: &[IndexItem]) -> bool {
    let now = chrono::Utc::now();
    view.sources.iter().all(|source| {
        items.iter().any(|item| {
            item.id == source.id
                && item.content_digest == source.digest
                && item.kind == "memory"
                && !item.sensitive
                && super::recall_validity::item_is_current_at(item, &now)
        })
    })
}

fn view_item(view: &GeneratedMemoryView, items: &[IndexItem]) -> IndexItem {
    let mut item = IndexItem::new(
        super::retention_view::document_entry_id(view.document, &view.content),
        view.content.clone(),
        IndexItemSource {
            kind: if view.document == UserMemoryDocumentId::Profile {
                "profile"
            } else {
                "soul"
            }
            .into(),
            revision: super::helpers::hash_parts(&[
                view.content.as_bytes(),
                serde_json::to_string(&view.sources)
                    .unwrap_or_default()
                    .as_bytes(),
            ]),
        },
    );
    item.trust_class = "candidate".into();
    item.confidence = VIEW_CONFIDENCE;
    item.valid_to = view
        .sources
        .iter()
        .filter_map(|source| {
            items
                .iter()
                .find(|item| item.id == source.id)
                .and_then(|item| item.valid_to.clone())
        })
        .min();
    item
}
