use chrono::{DateTime, Utc};

use super::helpers::hash_parts;
use super::{UserMemoryDocumentId, UserMemoryLearningState};

pub(super) fn document_entry_id(document: UserMemoryDocumentId, content: &str) -> String {
    let kind = match document {
        UserMemoryDocumentId::Memory => "memory",
        UserMemoryDocumentId::Profile => "profile",
        UserMemoryDocumentId::Soul => "soul",
    };
    let digest = hash_parts(&[document.file_name().as_bytes(), content.as_bytes()]);
    format!("iyw-{kind}-{}", &digest[..20])
}

pub(super) fn is_document_entry_id(id: &str) -> bool {
    ["iyw-profile-", "iyw-soul-"].iter().any(|prefix| {
        id.strip_prefix(prefix)
            .is_some_and(|suffix| super::is_lower_hex_string(suffix, 20))
    })
}

pub(super) fn active_document_content(
    document: UserMemoryDocumentId,
    content: &str,
    state: Option<&UserMemoryLearningState>,
) -> String {
    let Some(state) = state else {
        return content.to_string();
    };
    let now = Utc::now();
    if document == UserMemoryDocumentId::Memory {
        return content
            .lines()
            .filter(|line| {
                super::index_parse::parse_memory_line(line)
                    .is_none_or(|(id, value, _)| !expired(&id, &value, state, now))
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    content
        .split("\n\n")
        .filter(|paragraph| {
            let value = paragraph.trim();
            !expired(&document_entry_id(document, value), value, state, now)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn expired(id: &str, content: &str, state: &UserMemoryLearningState, now: DateTime<Utc>) -> bool {
    state.retention.get(id).is_some_and(|record| {
        record.content_digest == hash_parts(&[content.as_bytes()])
            && DateTime::parse_from_rfc3339(&record.expires_at)
                .is_ok_and(|expires| expires.with_timezone(&Utc) <= now)
    })
}
