use std::collections::{BTreeMap, BTreeSet};

use chrono::{SecondsFormat, Utc};

use crate::app_error::AppCommandError;

use super::{
    MemoryGovernancePreview, MemoryRetention, ResourceGeneration, UserMemoryDocumentId,
    UserMemoryGeneration, UserMemoryLearningState, UserMemoryService,
};

pub(super) async fn apply(
    service: &UserMemoryService,
    preview: &MemoryGovernancePreview,
    selected: &BTreeSet<String>,
) -> Result<(usize, usize, String), AppCommandError> {
    let policy = service.load_policy_unrecovered().await?;
    let settings = service.snapshot_locked(&policy)?;
    let previous_learning = service.read_learning_state()?;
    let snapshot = super::entry_catalog::management_snapshot(&settings, &previous_learning);
    let previous_document = service.read_document_resource(UserMemoryDocumentId::Memory)?;
    let mut next_learning = previous_learning.clone();
    let mut next_document = document_text(&previous_document);
    let stopped = stop_selected(preview, selected, &snapshot, &mut next_learning);
    let recovered = recover_selected(preview, selected, &mut next_learning, &mut next_document)?;
    curate_explicit_views(&snapshot, selected, &recovered, &mut next_learning);
    commit_changes(
        service,
        previous_document,
        next_document,
        previous_learning,
        next_learning.clone(),
    )
    .await?;
    service.schedule_index_refresh();
    let settings = service.snapshot_locked(&policy)?;
    let revision = super::entry_catalog::catalog_revision(&settings.revision, &next_learning)?;
    Ok((stopped, recovered.len(), revision))
}

fn stop_selected(
    preview: &MemoryGovernancePreview,
    selected: &BTreeSet<String>,
    snapshot: &super::index_types::IndexSnapshot,
    learning: &mut UserMemoryLearningState,
) -> usize {
    let ids = preview
        .recommendations
        .iter()
        .filter(|item| item.action == "stop" && selected.contains(&item.id))
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    for item in snapshot
        .items
        .iter()
        .filter(|item| ids.contains(item.id.as_str()))
    {
        learning.retention.insert(
            item.id.clone(),
            MemoryRetention {
                content_digest: item.content_digest.clone(),
                source_revision: item.source_revision.clone(),
                expires_at: now.clone(),
                reason: "Legacy task or scoped content stopped by reviewed governance".into(),
                updated_at: now.clone(),
            },
        );
    }
    ids.len()
}

fn recover_selected(
    preview: &MemoryGovernancePreview,
    selected: &BTreeSet<String>,
    learning: &mut UserMemoryLearningState,
    document: &mut String,
) -> Result<Vec<(String, String)>, AppCommandError> {
    let ids = preview
        .recommendations
        .iter()
        .filter(|item| item.action == "recover" && selected.contains(&item.id))
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut recovered = Vec::new();
    for candidate in learning
        .candidates
        .iter_mut()
        .filter(|item| ids.contains(item.id.as_str()))
    {
        let entry_id = super::helpers::memory_entry_id(&candidate.content);
        if !document.contains(&entry_id) {
            append_candidate(document, candidate, &entry_id)?;
        }
        candidate.status = super::UserMemoryCandidateStatus::Confirmed;
        candidate.resolved_content = Some(candidate.content.clone());
        candidate.confirmed_memory_entry_id = Some(entry_id.clone());
        candidate.superseded_by_candidate_id = None;
        candidate.superseded_by_memory_entry_id = None;
        candidate.resolved_at = Some(Utc::now().to_rfc3339());
        recovered.push((entry_id, candidate.content.clone()));
    }
    Ok(recovered)
}

fn curate_explicit_views(
    snapshot: &super::index_types::IndexSnapshot,
    stopped: &BTreeSet<String>,
    recovered: &[(String, String)],
    learning: &mut UserMemoryLearningState,
) {
    let stable = snapshot
        .items
        .iter()
        .filter(|item| item.kind == "memory" && !item.sensitive && !stopped.contains(&item.id));
    let sources = stable
        .map(|item| {
            (
                item.id.clone(),
                item.content.clone(),
                item.content_digest.clone(),
            )
        })
        .chain(recovered.iter().map(|(id, content)| {
            (
                id.clone(),
                content.clone(),
                super::helpers::hash_parts(&[content.as_bytes()]),
            )
        }));
    for (id, content, digest) in sources {
        let Some(document) = projection_document(&content) else {
            continue;
        };
        if learning
            .generated_views
            .iter()
            .any(|view| view.document == document && view.content == content)
        {
            continue;
        }
        learning.generated_views.push(super::GeneratedMemoryView {
            document,
            content,
            sources: vec![super::GeneratedViewSource { id, digest }],
        });
    }
    learning.generated_views.truncate(24);
    learning.generated_views_input_digest = None;
}

fn projection_document(content: &str) -> Option<UserMemoryDocumentId> {
    if content.contains("用户姓") || content.contains("职业") || content.contains("Windows 环境")
    {
        Some(UserMemoryDocumentId::Profile)
    } else if content.contains("用户希望以后")
        || content.contains("版本说明")
        || content.contains("markdown 形式")
    {
        Some(UserMemoryDocumentId::Soul)
    } else {
        None
    }
}

fn append_candidate(
    document: &mut String,
    candidate: &super::UserMemoryCandidate,
    entry_id: &str,
) -> Result<(), AppCommandError> {
    let observation = candidate.observations.last().ok_or_else(|| {
        AppCommandError::configuration_invalid("Recoverable candidate has no source")
    })?;
    if !document.is_empty() && !document.ends_with('\n') {
        document.push('\n');
    }
    document.push_str(&format!(
        "- [{}] [{}] {} <!-- {} -->\n",
        observation.observed_at,
        super::helpers::agent_memory_label(observation.agent_type),
        candidate.content,
        entry_id
    ));
    super::helpers::validate_document_content(document)
}

async fn commit_changes(
    service: &UserMemoryService,
    previous_document: ResourceGeneration<String>,
    next_document: String,
    previous_learning: UserMemoryLearningState,
    next_learning: UserMemoryLearningState,
) -> Result<(), AppCommandError> {
    let previous = generation(previous_document.clone(), previous_learning)?;
    let next = UserMemoryGeneration {
        policy: None,
        documents: BTreeMap::from([(
            UserMemoryDocumentId::Memory,
            super::transaction::document_resource(next_document),
        )]),
        candidate_state: Some(super::transaction::candidate_resource(next_learning)?),
    };
    if service.active_authority().is_some() {
        service
            .commit_authority_change(&previous, &next, None)
            .await
    } else {
        service.execute_transaction(previous, next).await
    }
}

fn generation(
    document: ResourceGeneration<String>,
    learning: UserMemoryLearningState,
) -> Result<UserMemoryGeneration, AppCommandError> {
    Ok(UserMemoryGeneration {
        policy: None,
        documents: BTreeMap::from([(UserMemoryDocumentId::Memory, document)]),
        candidate_state: Some(super::transaction::candidate_resource(learning)?),
    })
}

fn document_text(resource: &ResourceGeneration<String>) -> String {
    match resource {
        ResourceGeneration::Present { value, .. } => value.clone(),
        ResourceGeneration::Absent => String::new(),
    }
}
