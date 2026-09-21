use super::authority_types::{AuthorityMarker, AuthoritySnapshot};
use super::{authority_sql as sql, structured_file, ResourceGeneration, UserMemoryService};
use crate::app_error::AppCommandError;
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) const MARKER_FILE: &str = ".memory-authority.json";
const MARKER_LIMIT: usize = 16_384;

pub(super) fn marker(root: &Path) -> Result<Option<AuthorityMarker>, AppCommandError> {
    structured_file::read_json_optional(root, MARKER_FILE, MARKER_LIMIT)
}

pub(super) fn validate_marker(
    root: &Path,
    snapshot: Option<&AuthoritySnapshot>,
) -> Result<(), AppCommandError> {
    if let Some(marker) = marker(root)? {
        if !snapshot.is_some_and(|snapshot| {
            snapshot.mode == "active"
                && snapshot.store_id == marker.store_id
                && snapshot.epoch >= marker.epoch
                && (snapshot.epoch > marker.epoch || snapshot.digest == marker.digest)
        }) {
            return Err(super::helpers::conflict("Memory database is missing or older than its authority marker; restore requires reconciliation"));
        }
    }
    if snapshot.is_some_and(|value| value.mode == "active") && marker(root)?.is_none() {
        return Err(super::helpers::conflict(
            "Active memory authority marker is missing; reconciliation required",
        ));
    }
    Ok(())
}

pub(super) fn write_fence(
    root: &Path,
    snapshot: &AuthoritySnapshot,
) -> Result<(), AppCommandError> {
    let exports = match marker(root)? {
        Some(previous) => previous.exports,
        None => export_contents(snapshot)?
            .keys()
            .map(|name| Ok((name.clone(), read_export(root, name)?.as_deref().map(hash))))
            .collect::<Result<_, AppCommandError>>()?,
    };
    structured_file::write_json_atomic(
        root,
        MARKER_FILE,
        &AuthorityMarker {
            store_id: snapshot.store_id.clone(),
            epoch: snapshot.epoch,
            digest: snapshot.digest.clone(),
            exports,
        },
    )
}

pub(super) fn export_contents(
    snapshot: &AuthoritySnapshot,
) -> Result<BTreeMap<String, Option<String>>, AppCommandError> {
    let mut files = BTreeMap::new();
    for (id, resource) in &snapshot.data.documents {
        files.insert(
            id.file_name().into(),
            match resource {
                ResourceGeneration::Present { value, .. } => Some(value.clone()),
                ResourceGeneration::Absent => None,
            },
        );
    }
    files.insert(
        super::USER_MEMORY_CANDIDATE_FILE.into(),
        match &snapshot.data.learning {
            ResourceGeneration::Present { value, .. } => Some(super::authority::encode(value)?),
            ResourceGeneration::Absent => None,
        },
    );
    Ok(files)
}

pub(super) fn external_changes(
    root: &Path,
    snapshot: &AuthoritySnapshot,
) -> Result<Vec<String>, AppCommandError> {
    let Some(previous) = marker(root)? else {
        return Ok(Vec::new());
    };
    let next = export_contents(snapshot)?;
    let mut conflicts = Vec::new();
    for (name, content) in next {
        let current = read_export(root, &name)?;
        let current_digest = current.as_deref().map(hash);
        let next_digest = content.as_deref().map(hash);
        if current_digest != next_digest && previous.exports.get(&name) != Some(&current_digest) {
            conflicts.push(name);
        }
    }
    Ok(conflicts)
}

impl UserMemoryService {
    pub(super) async fn export_authority_locked(
        &self,
        snapshot: &AuthoritySnapshot,
    ) -> Result<(), AppCommandError> {
        let root = self.resolved_root()?;
        if !external_changes(root, snapshot)?.is_empty() {
            return Err(super::helpers::conflict(
                "Compatibility memory files were edited externally; original edits were preserved",
            ));
        }
        let files = export_contents(snapshot)?;
        let exports = files
            .iter()
            .map(|(name, text)| (name.clone(), text.as_deref().map(hash)))
            .collect();
        for (name, content) in files {
            if let Some(content) = content {
                if read_export(root, &name)?.as_deref() != Some(&content) {
                    structured_file::write_bytes_atomic(root, &name, content.as_bytes())?;
                }
            } else {
                structured_file::remove_optional(root, &name)?;
            }
        }
        structured_file::write_json_atomic(
            root,
            MARKER_FILE,
            &AuthorityMarker {
                store_id: snapshot.store_id.clone(),
                epoch: snapshot.epoch,
                digest: snapshot.digest.clone(),
                exports,
            },
        )?;
        self.complete_projection(snapshot.epoch, "files").await
    }

    pub(super) async fn complete_projection(
        &self,
        epoch: i64,
        kind: &str,
    ) -> Result<(), AppCommandError> {
        sql::execute(&self.db,"UPDATE memory_projection_job SET state='done',updated_at=? WHERE root_key=? AND epoch=? AND kind=? AND state='pending'",
            vec![chrono::Utc::now().to_rfc3339().into(),self.authority_key()?.into(),epoch.into(),kind.into()]).await?;
        Ok(())
    }

    pub(super) async fn retry_authority_export(&self) -> Result<(), AppCommandError> {
        let (_guard, _file) = self.acquire_locks().await?;
        if let Some(snapshot) = self.active_authority() {
            self.export_authority_locked(&snapshot).await?;
        }
        Ok(())
    }
}

fn hash(text: &str) -> String {
    super::helpers::hash_parts(&[text.as_bytes()])
}

pub(super) fn read_export(root: &Path, name: &str) -> Result<Option<String>, AppCommandError> {
    match structured_file::read_bounded_utf8(&root.join(name), 16_777_216) {
        Ok(text) => Ok(Some(text)),
        Err(structured_file::StructuredReadError::Missing) => Ok(None),
        Err(error) => Err(
            AppCommandError::configuration_invalid("Cannot read memory export")
                .with_detail(error.to_string()),
        ),
    }
}
