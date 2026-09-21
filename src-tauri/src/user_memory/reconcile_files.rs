use super::authority_types::AuthoritySnapshot;
use super::{
    authority_export as export, helpers, MemoryFileConflict, ResourceGeneration,
    UserMemoryDocumentId, UserMemoryGeneration, UserMemoryService,
};
use crate::app_error::AppCommandError;
use std::collections::BTreeMap;

pub(super) fn conflicts(
    service: &UserMemoryService,
    snapshot: &AuthoritySnapshot,
) -> Result<Vec<MemoryFileConflict>, AppCommandError> {
    let names = export::external_changes(service.resolved_root()?, snapshot)?;
    let files = export::export_contents(snapshot)?;
    names
        .into_iter()
        .map(|name| {
            let current = files.get(&name).cloned().flatten();
            let external = export::read_export(service.resolved_root()?, &name)?;
            let document = UserMemoryDocumentId::ALL
                .into_iter()
                .find(|id| id.file_name() == name);
            let error = document.map(|id| {
                prepare_import(
                    service,
                    snapshot,
                    (id, external.as_deref().unwrap_or_default()),
                )
                .err()
            });
            let redacted = current
                .iter()
                .chain(external.iter())
                .any(|text| helpers::contains_potential_secret(text));
            Ok(MemoryFileConflict {
                name,
                document,
                current_digest: digest(current.as_deref()),
                external_digest: digest(external.as_deref()),
                current: current.filter(|_| !redacted),
                external: external.filter(|_| !redacted),
                redacted,
                import_allowed: !redacted && error.as_ref().is_some_and(Option::is_none),
                import_error: error.flatten().map(|error| error.message),
            })
        })
        .collect()
}

pub(super) fn prepare_import(
    service: &UserMemoryService,
    snapshot: &AuthoritySnapshot,
    input: (UserMemoryDocumentId, &str),
) -> Result<(UserMemoryGeneration, UserMemoryGeneration), AppCommandError> {
    let (id, content) = input;
    validate_external_document(snapshot, id, content)?;
    let current = &snapshot.data.documents[&id];
    let mut learning = match &snapshot.data.learning {
        ResourceGeneration::Present { value, .. } => value.clone(),
        ResourceGeneration::Absent => Default::default(),
    };
    let mut data = snapshot.data.clone();
    data.documents
        .insert(id, super::transaction::document_resource(content.into()));
    super::authority_validation::validate_import(&data)?;
    let before = service.authority_index_snapshot(&snapshot.data)?;
    let after = service.authority_index_snapshot(&data)?;
    super::index_integrity::validate_snapshot_identities(&after)
        .map_err(super::index_checkpoint::database_error)?;
    pause_imports(&before, &after, &mut learning);
    let previous = UserMemoryGeneration {
        policy: None,
        documents: BTreeMap::from([(id, current.clone())]),
        candidate_state: Some(snapshot.data.learning.clone()),
    };
    let next = UserMemoryGeneration {
        policy: None,
        documents: BTreeMap::from([(id, super::transaction::document_resource(content.into()))]),
        candidate_state: Some(super::transaction::candidate_resource(learning)?),
    };
    Ok((previous, next))
}

fn pause_imports(
    before: &super::index_types::IndexSnapshot,
    after: &super::index_types::IndexSnapshot,
    learning: &mut super::UserMemoryLearningState,
) {
    for item in &after.items {
        if before
            .items
            .iter()
            .any(|old| old.id == item.id && old.content_digest == item.content_digest)
        {
            continue;
        }
        learning.retention.insert(
            item.id.clone(),
            super::MemoryRetention {
                content_digest: item.content_digest.clone(),
                source_revision: item.source_revision.clone(),
                expires_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                updated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                reason: "external_import_pending_review".into(),
            },
        );
    }
}

fn validate_external_document(
    snapshot: &AuthoritySnapshot,
    id: UserMemoryDocumentId,
    content: &str,
) -> Result<(), AppCommandError> {
    helpers::ensure_manual_document_write_allowed(&snapshot.data.policy, id)?;
    helpers::validate_document_update_content(content)?;
    if id != UserMemoryDocumentId::Memory {
        return Ok(());
    }
    if content
        .lines()
        .filter_map(super::index_parse::parse_memory_line)
        .any(|(id, content, _)| id != helpers::memory_entry_id(&content))
    {
        return Err(AppCommandError::invalid_input(
            "Changed memory markers require the dedicated correction action",
        ));
    }
    let old = match &snapshot.data.documents[&id] {
        ResourceGeneration::Present { value, .. } => value.as_str(),
        ResourceGeneration::Absent => "",
    };
    if let ResourceGeneration::Present { value, .. } = &snapshot.data.learning {
        if !super::candidate_references::preserves_referenced_memory_entries(value, old, content) {
            return Err(AppCommandError::invalid_input(
                "Candidate-backed entries require the dedicated correction or retirement action",
            ));
        }
    }
    Ok(())
}

pub(super) fn digest(text: Option<&str>) -> Option<String> {
    text.map(|text| helpers::hash_parts(&[text.as_bytes()]))
}
