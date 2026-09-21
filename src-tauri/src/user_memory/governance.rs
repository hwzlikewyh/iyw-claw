use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

use super::{UserMemoryDocumentId, UserMemoryService};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGovernancePreview {
    pub revision: String,
    pub recommendations: Vec<MemoryGovernanceRecommendation>,
    pub retained_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGovernanceRecommendation {
    pub id: String,
    pub document: UserMemoryDocumentId,
    pub content: String,
    pub reason: String,
    pub action: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyMemoryGovernanceRequest {
    pub expected_revision: String,
    pub ids: Vec<String>,
    pub acknowledged: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyMemoryGovernanceResult {
    pub stopped: usize,
    pub recovered: usize,
    pub revision: String,
}

impl UserMemoryService {
    pub async fn preview_memory_governance(
        &self,
    ) -> Result<MemoryGovernancePreview, AppCommandError> {
        let (_guard, _file) = self.acquire_locks().await?;
        self.governance_preview_locked().await
    }

    pub async fn apply_memory_governance(
        &self,
        request: ApplyMemoryGovernanceRequest,
    ) -> Result<ApplyMemoryGovernanceResult, AppCommandError> {
        validate_apply_request(&request)?;
        let (_guard, _file) = self.acquire_locks().await?;
        let preview = self.governance_preview_locked().await?;
        if preview.revision != request.expected_revision {
            return Err(super::helpers::conflict(
                "Memory governance preview changed; refresh before applying",
            ));
        }
        let selected = validate_selection(request.ids, &preview)?;
        let result = super::governance_apply::apply(self, &preview, &selected).await?;
        tracing::info!(
            stopped = result.0,
            recovered = result.1,
            "[memory-governance] reviewed legacy entries stopped"
        );
        Ok(ApplyMemoryGovernanceResult {
            stopped: result.0,
            recovered: result.1,
            revision: result.2,
        })
    }

    async fn governance_preview_locked(&self) -> Result<MemoryGovernancePreview, AppCommandError> {
        let policy = self.load_policy_unrecovered().await?;
        let settings = self.snapshot_locked(&policy)?;
        let learning = self.read_learning_state()?;
        let revision = super::entry_catalog::catalog_revision(&settings.revision, &learning)?;
        let snapshot = super::entry_catalog::management_snapshot(&settings, &learning);
        let active = snapshot.items.iter().filter(|item| {
            item.kind == "memory"
                && item.scope_type == "global"
                && super::recall_validity::item_is_current_at(item, &Utc::now())
        });
        let mut retained_count = 0;
        let mut recommendations = Vec::new();
        for item in active {
            match governance_reason(&item.content) {
                Some(reason) => recommendations.push(MemoryGovernanceRecommendation {
                    id: item.id.clone(),
                    document: UserMemoryDocumentId::Memory,
                    content: item.content.clone(),
                    reason: reason.into(),
                    action: "stop".into(),
                }),
                None => retained_count += 1,
            }
        }
        recommendations.extend(recovery_recommendations(&learning, &snapshot));
        Ok(MemoryGovernancePreview {
            revision,
            recommendations,
            retained_count,
        })
    }
}

fn recovery_recommendations(
    learning: &super::UserMemoryLearningState,
    snapshot: &super::index_types::IndexSnapshot,
) -> Vec<MemoryGovernanceRecommendation> {
    learning
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.status.is_terminal()
                && candidate.resolved_content.as_deref() != Some(candidate.content.as_str())
                && candidate
                    .confirmed_memory_entry_id
                    .as_deref()
                    .is_some_and(|id| {
                        snapshot
                            .items
                            .iter()
                            .any(|item| item.id == id && item.content != candidate.content)
                    })
        })
        .map(|candidate| MemoryGovernanceRecommendation {
            id: candidate.id.clone(),
            document: UserMemoryDocumentId::Memory,
            content: candidate.content.clone(),
            reason: "mislinked_candidate".into(),
            action: "recover".into(),
        })
        .collect()
}

fn validate_apply_request(request: &ApplyMemoryGovernanceRequest) -> Result<(), AppCommandError> {
    if !request.acknowledged || request.ids.is_empty() || request.ids.len() > 100 {
        return Err(AppCommandError::invalid_input(
            "Select and acknowledge memory governance recommendations",
        ));
    }
    Ok(())
}

fn validate_selection(
    ids: Vec<String>,
    preview: &MemoryGovernancePreview,
) -> Result<std::collections::BTreeSet<String>, AppCommandError> {
    let selected = ids.into_iter().collect::<std::collections::BTreeSet<_>>();
    if selected.is_empty()
        || selected.iter().any(|id| {
            !preview
                .recommendations
                .iter()
                .any(|recommendation| recommendation.id == *id)
        })
    {
        return Err(AppCommandError::invalid_input(
            "Governance selection is not part of the current preview",
        ));
    }
    Ok(selected)
}

fn governance_reason(content: &str) -> Option<&'static str> {
    let trimmed = content.trim();
    if trimmed.contains("iyw-claw://session/") {
        return Some("conversation_task_link");
    }
    if trimmed.chars().count() <= 12 {
        return Some("context_poor_fragment");
    }
    let task_prefixes = ["参考这个", "根据上传", "不要出现", "挂在"];
    if task_prefixes
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
    {
        return Some(if trimmed.contains('【') {
            "quoted_material"
        } else {
            "task_scoped_instruction"
        });
    }
    None
}
