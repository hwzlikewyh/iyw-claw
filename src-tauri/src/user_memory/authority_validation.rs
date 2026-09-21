use super::authority_types::AuthorityData;
use super::{ResourceGeneration, UserMemoryDocumentId, UserMemoryGeneration};
use crate::app_error::AppCommandError;

pub(super) fn validate(data: &AuthorityData) -> Result<(), AppCommandError> {
    if data.documents.len() != UserMemoryDocumentId::ALL.len()
        || UserMemoryDocumentId::ALL
            .iter()
            .any(|id| !data.documents.contains_key(id) || !data.policy.documents.contains_key(id))
    {
        return Err(AppCommandError::configuration_invalid(
            "Incomplete memory authority",
        ));
    }
    super::transaction::validate_generation(&UserMemoryGeneration {
        policy: Some(data.policy.clone()),
        documents: data.documents.clone(),
        candidate_state: Some(data.learning.clone()),
    })
}

pub(super) fn validate_import(data: &AuthorityData) -> Result<(), AppCommandError> {
    validate(data)?;
    if let Some(ResourceGeneration::Present { value, .. }) =
        data.documents.get(&UserMemoryDocumentId::Memory)
    {
        if value
            .lines()
            .any(super::migration_reconcile::is_unparsed_memory_line)
        {
            return Err(AppCommandError::invalid_input(
                "Memory contains unparsed lines; reconcile the migration preview first",
            ));
        }
    }
    Ok(())
}
