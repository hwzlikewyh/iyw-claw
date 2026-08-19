#[derive(Debug, Clone)]
pub(super) struct IndexSnapshot {
    pub source_key: String,
    pub source_digest: String,
    pub items: Vec<IndexItem>,
    pub relations: Vec<IndexRelation>,
}

#[derive(Debug, Clone)]
pub(super) struct IndexItem {
    pub id: String,
    pub kind: String,
    pub trust_class: String,
    pub scope_type: String,
    pub scope_key: String,
    pub content: String,
    pub content_digest: String,
    pub confidence: i64,
    pub importance: f64,
    pub sensitive: bool,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub source_revision: String,
    pub aliases: Vec<IndexAlias>,
    pub evidence: Vec<IndexEvidence>,
}

#[derive(Debug, Clone)]
pub(super) struct IndexAlias {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub(super) struct IndexEvidence {
    pub source_kind: String,
    pub source_id: String,
    pub conversation_id: Option<String>,
    pub turn_nonce: i64,
    pub excerpt_digest: String,
    pub observed_at: String,
}

#[derive(Debug, Clone)]
pub(super) struct IndexRelation {
    pub source_id: String,
    pub relation: String,
    pub target_id: String,
    pub confidence: i64,
    pub created_at: String,
}

pub(super) struct IndexItemSource {
    pub kind: String,
    pub revision: String,
}

pub(super) fn normalize_alias(value: &str) -> String {
    value
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

impl IndexItem {
    pub(super) fn new(id: String, content: String, source: IndexItemSource) -> Self {
        let content_digest = super::helpers::hash_parts(&[content.as_bytes()]);
        Self {
            id: id.clone(),
            kind: source.kind,
            trust_class: "host_confirmed".to_string(),
            scope_type: "global".to_string(),
            scope_key: String::new(),
            content,
            content_digest,
            confidence: 100,
            importance: 0.5,
            sensitive: false,
            valid_from: None,
            valid_to: None,
            source_revision: source.revision,
            aliases: vec![IndexAlias {
                kind: "stable_id".to_string(),
                value: id,
            }],
            evidence: Vec::new(),
        }
    }

    pub(super) fn add_alias(&mut self, kind: &str, value: impl Into<String>) {
        let value = value.into();
        let normalized = normalize_alias(&value);
        if normalized.is_empty()
            || self
                .aliases
                .iter()
                .any(|alias| normalize_alias(&alias.value) == normalized)
        {
            return;
        }
        self.aliases.push(IndexAlias {
            kind: kind.to_string(),
            value,
        });
    }

    pub(super) fn add_evidence(&mut self, evidence: IndexEvidence) {
        let duplicate = self.evidence.iter().any(|existing| {
            existing.source_kind == evidence.source_kind
                && existing.source_id == evidence.source_id
                && existing.turn_nonce == evidence.turn_nonce
        });
        if !duplicate {
            self.evidence.push(evidence);
        }
    }
}
