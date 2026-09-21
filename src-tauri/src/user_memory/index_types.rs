#[derive(Debug, Clone)]
pub(super) struct IndexSnapshot {
    pub source_key: String,
    pub source_digest: String,
    pub items: Vec<IndexItem>,
    pub relations: Vec<IndexRelation>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct IndexAlias {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct IndexEvidence {
    pub source_kind: String,
    pub source_id: String,
    pub conversation_id: Option<String>,
    pub turn_nonce: i64,
    pub excerpt_digest: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

pub(super) fn semantic_intent_key(value: &str) -> Option<&'static str> {
    let lower = value.to_lowercase();
    let weekly_report = ["周报", "weekly report"]
        .iter()
        .any(|token| lower.contains(token));
    let chat_delivery = [
        "不用输出附件",
        "不需要附件",
        "不要附件",
        "不用附件",
        "无附件",
        "不用文件",
        "不需要文件",
        "不要文件",
        "直接在聊天",
        "聊天中输出",
        "聊天里输出",
    ]
    .iter()
    .any(|token| lower.contains(token));
    let image = ["图片", "图像", "作图", "生图", "画图", "image"]
        .iter()
        .any(|token| lower.contains(token));
    let generate = [
        "生成",
        "作图",
        "生图",
        "做一张",
        "画一张",
        "create",
        "generate",
    ]
    .iter()
    .any(|token| lower.contains(token));
    let direct = ["直接", "马上", "立刻", "无需确认", "不用确认", "direct"]
        .iter()
        .any(|token| lower.contains(token));
    let display = ["展示", "显示", "markdown", "远程 url", "聊天里"]
        .iter()
        .any(|token| lower.contains(token));
    if weekly_report && chat_delivery {
        Some("intent:weekly_report:chat_delivery")
    } else if image && display {
        Some("intent:image_display:remote_url")
    } else if image && generate && direct {
        Some("intent:image_generation:direct_execution")
    } else {
        None
    }
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
