use crate::app_error::AppCommandError;

use super::{fs, journal, UserMemoryService};

impl UserMemoryService {
    pub(super) async fn recover_pending_transaction(&self) -> Result<(), AppCommandError> {
        let root = self.resolved_root()?;
        match journal::read(root)? {
            None => Ok(()),
            Some(journal::PendingJournal::Current(transaction)) => {
                self.recover_transaction(&transaction).await
            }
            Some(journal::PendingJournal::Legacy(pending)) => {
                self.recover_legacy_pending_update(&pending).await
            }
        }
    }

    async fn recover_legacy_pending_update(
        &self,
        pending: &journal::LegacyPendingUpdate,
    ) -> Result<(), AppCommandError> {
        validate_legacy_pending_update(pending)?;
        let current_policy = self.load_policy_unrecovered().await?;
        if current_policy == pending.next_policy
            && self.legacy_documents_match(&pending.next_documents)?
        {
            return journal::remove(self.resolved_root()?);
        }
        if current_policy != pending.previous_policy {
            return Err(super::transaction::transaction_invalid(
                "legacy journal does not match stored policy",
            ));
        }
        self.validate_legacy_document_membership(pending)?;
        let root = self.resolved_root()?;
        for id in pending.previous_documents.keys() {
            fs::ensure_document_writable_optional(root, *id)?;
        }
        for (id, content) in &pending.previous_documents {
            fs::apply_document_generation(root, *id, Some(content))?;
        }
        journal::remove(root)
    }

    fn legacy_documents_match(
        &self,
        expected: &std::collections::BTreeMap<super::UserMemoryDocumentId, String>,
    ) -> Result<bool, AppCommandError> {
        let root = self.resolved_root()?;
        for (id, expected) in expected {
            if fs::read_document_optional(root, *id)?.as_deref() != Some(expected.as_str()) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn validate_legacy_document_membership(
        &self,
        pending: &journal::LegacyPendingUpdate,
    ) -> Result<(), AppCommandError> {
        let root = self.resolved_root()?;
        for (id, previous) in &pending.previous_documents {
            let current = fs::read_document_optional(root, *id)?;
            if current.as_ref() != Some(previous)
                && current.as_ref() != Some(&pending.next_documents[id])
            {
                return Err(super::transaction::transaction_invalid(
                    "current document mismatches legacy journal",
                ));
            }
        }
        Ok(())
    }
}

fn validate_legacy_pending_update(
    pending: &journal::LegacyPendingUpdate,
) -> Result<(), AppCommandError> {
    if !pending
        .previous_documents
        .keys()
        .eq(pending.next_documents.keys())
    {
        return Err(super::transaction::transaction_invalid(
            "legacy journal document sets differ",
        ));
    }
    for content in pending
        .previous_documents
        .values()
        .chain(pending.next_documents.values())
    {
        super::helpers::validate_document_content(content)?;
    }
    Ok(())
}
