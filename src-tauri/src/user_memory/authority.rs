use super::authority_types::{AuthorityData, AuthoritySnapshot};
use super::{
    authority_sql as sql, candidate_store, ResourceGeneration, UserMemoryGeneration,
    UserMemoryLearningState, UserMemoryService,
};
use crate::app_error::AppCommandError;

impl UserMemoryService {
    pub(super) fn authority_key(&self) -> Result<String, AppCommandError> {
        let path = crate::commands::skill_inventory::workspace_key(Some(
            self.resolved_root()?.to_string_lossy().as_ref(),
        ));
        Ok(super::helpers::hash_parts(&[
            b"memory-owner-v1",
            path.as_bytes(),
        ]))
    }

    pub(super) fn authority_snapshot(&self) -> Option<AuthoritySnapshot> {
        self.authority
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(super) fn active_authority(&self) -> Option<AuthoritySnapshot> {
        self.authority_snapshot()
            .filter(|snapshot| snapshot.mode == "active")
    }

    pub(super) async fn load_authority_locked(&self) -> Result<(), AppCommandError> {
        self.recover_memory_restore_locked().await?;
        let snapshot = sql::load(&self.db, &self.authority_key()?).await?;
        let snapshot = self.recover_authority_commit(snapshot).await?;
        super::authority_export::validate_marker(self.resolved_root()?, snapshot.as_ref())?;
        *self
            .authority
            .write()
            .unwrap_or_else(|error| error.into_inner()) = snapshot;
        Ok(())
    }

    pub(super) fn read_learning_optional(
        &self,
    ) -> Result<Option<UserMemoryLearningState>, AppCommandError> {
        if let Some(snapshot) = self.active_authority() {
            return Ok(match snapshot.data.learning {
                ResourceGeneration::Present { value, .. } => Some(value),
                ResourceGeneration::Absent => None,
            });
        }
        candidate_store::read_optional(self.resolved_root()?)
    }

    pub(super) fn read_learning_state(&self) -> Result<UserMemoryLearningState, AppCommandError> {
        Ok(self.read_learning_optional()?.unwrap_or_default())
    }

    pub(super) async fn persist_learning_state(
        &self,
        state: &UserMemoryLearningState,
    ) -> Result<(), AppCommandError> {
        if self.active_authority().is_none() {
            return candidate_store::write_state(self.resolved_root()?, state);
        }
        let previous = self
            .read_learning_optional()?
            .map(super::transaction::candidate_resource)
            .transpose()?
            .unwrap_or(ResourceGeneration::Absent);
        self.commit_authority_change(
            &UserMemoryGeneration {
                policy: None,
                documents: Default::default(),
                candidate_state: Some(previous),
            },
            &UserMemoryGeneration {
                policy: None,
                documents: Default::default(),
                candidate_state: Some(super::transaction::candidate_resource(state.clone())?),
            },
            None,
        )
        .await
    }

    pub(super) async fn commit_authority_change(
        &self,
        previous: &UserMemoryGeneration,
        next: &UserMemoryGeneration,
        identity: Option<(&str, &str)>,
    ) -> Result<(), AppCommandError> {
        let current = self
            .active_authority()
            .ok_or_else(|| super::helpers::conflict("Memory authority is not active"))?;
        super::transaction::validate_participation(previous, next)?;
        validate_change(&current.data, previous)?;
        let data = apply_change(current.data.clone(), next);
        super::authority_validation::validate(&data)?;
        let digest = super::authority_types::digest(&data)?;
        if current.digest == digest {
            return Ok(());
        }
        let epoch = current.epoch + 1;
        let snapshot = AuthoritySnapshot {
            data,
            digest,
            epoch,
            ..current
        };
        self.save_authority_transaction(&snapshot, identity).await?;
        *self
            .authority
            .write()
            .unwrap_or_else(|error| error.into_inner()) = Some(snapshot.clone());
        if let Err(error) = self.export_authority_locked(&snapshot).await {
            tracing::warn!(code=?error.code, epoch, "[memory-authority] saved; compatibility export deferred");
        }
        self.schedule_index_refresh();
        Ok(())
    }

    pub(super) fn authority_index_snapshot(
        &self,
        data: &AuthorityData,
    ) -> Result<super::index_types::IndexSnapshot, AppCommandError> {
        let mut settings = super::index_source::readonly_snapshot(self, &data.policy)?;
        settings.enabled = true;
        for (id, resource) in &data.documents {
            let content = match resource {
                ResourceGeneration::Present { value, .. } => value.clone(),
                ResourceGeneration::Absent => String::new(),
            };
            settings.documents.insert(
                *id,
                super::settings_projection::readable_document_snapshot(
                    self.resolved_root()?,
                    &data.policy,
                    *id,
                    content,
                ),
            );
        }
        for document in settings.documents.values_mut() {
            document.enabled = true;
        }
        settings.revision = super::authority_types::digest(data)?;
        let learning = match &data.learning {
            ResourceGeneration::Present { value, .. } => Some(value),
            ResourceGeneration::Absent => None,
        };
        Ok(
            self.scope_index_snapshot(super::index_parse::build_index_snapshot(
                &settings, learning,
            )),
        )
    }
}

pub(super) async fn queue_projections<C: sea_orm::ConnectionTrait>(
    db: &C,
    key: &str,
    epoch: i64,
) -> Result<(), AppCommandError> {
    let now = chrono::Utc::now().to_rfc3339();
    sql::execute(db,"UPDATE memory_projection_job SET state='superseded', updated_at=? WHERE root_key=? AND state='pending'",vec![now.clone().into(),key.into()]).await?;
    for kind in ["files", "fts", "vector"] {
        sql::execute(db,"INSERT INTO memory_projection_job (root_key,epoch,kind,state,updated_at) VALUES (?,?,?,'pending',?)",vec![key.into(),epoch.into(),kind.into(),now.clone().into()]).await?;
    }
    Ok(())
}

pub(super) fn encode<T: serde::Serialize>(value: &T) -> Result<String, AppCommandError> {
    serde_json::to_string(value).map_err(|error| {
        AppCommandError::configuration_invalid("Cannot serialize memory authority")
            .with_detail(error.to_string())
    })
}

pub(super) fn validate_change(
    data: &AuthorityData,
    expected: &UserMemoryGeneration,
) -> Result<(), AppCommandError> {
    if expected
        .policy
        .as_ref()
        .is_some_and(|policy| policy != &data.policy)
        || expected
            .candidate_state
            .as_ref()
            .is_some_and(|learning| learning != &data.learning)
        || expected
            .documents
            .iter()
            .any(|(id, value)| data.documents.get(id) != Some(value))
    {
        return Err(super::helpers::conflict("Memory source revision changed"));
    }
    Ok(())
}

pub(super) fn apply_change(
    mut data: AuthorityData,
    change: &UserMemoryGeneration,
) -> AuthorityData {
    if let Some(policy) = &change.policy {
        data.policy = policy.clone();
    }
    if let Some(learning) = &change.candidate_state {
        data.learning = learning.clone();
    }
    data.documents.extend(change.documents.clone());
    data
}
