use serde::{Deserialize, Serialize};

use super::{
    GeneratedMemoryView, GeneratedViewSource, UserMemoryDocumentId, UserMemoryLearningState,
};
use crate::app_error::AppCommandError;

const MAX_OVERRIDES: usize = 256;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneratedViewOverride {
    pub document: UserMemoryDocumentId,
    pub sources: Vec<GeneratedViewSource>,
}

pub(super) fn block_view(
    state: &mut UserMemoryLearningState,
    view: &GeneratedMemoryView,
) -> Result<(), AppCommandError> {
    let entry = GeneratedViewOverride {
        document: view.document,
        sources: view.sources.clone(),
    };
    if !state.generated_overrides.contains(&entry) {
        if state.generated_overrides.len() >= MAX_OVERRIDES {
            return Err(AppCommandError::invalid_input(
                "Generated memory override limit reached",
            ));
        }
        state.generated_overrides.push(entry);
    }
    Ok(())
}

pub(super) fn permits(state: &UserMemoryLearningState, view: &GeneratedMemoryView) -> bool {
    !state.generated_overrides.iter().any(|entry| {
        entry.document == view.document
            && entry
                .sources
                .iter()
                .any(|blocked| view.sources.contains(blocked))
    })
}

pub(super) fn unblock_view(state: &mut UserMemoryLearningState, view: &GeneratedMemoryView) {
    state
        .generated_overrides
        .retain(|entry| entry.document != view.document || entry.sources != view.sources);
}

pub(super) fn validate(entries: &[GeneratedViewOverride]) -> Result<(), AppCommandError> {
    if entries.len() > MAX_OVERRIDES {
        return Err(AppCommandError::invalid_input(
            "Generated memory override limit exceeded",
        ));
    }
    for entry in entries {
        super::generated_views::validate_views(&[GeneratedMemoryView {
            document: entry.document,
            content: "User override".into(),
            sources: entry.sources.clone(),
        }])?;
    }
    Ok(())
}
