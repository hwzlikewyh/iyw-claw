use std::path::Path;

use super::helpers::hash_parts;
use super::index_types::IndexSnapshot;
use super::{UserMemoryRecallScope, UserMemoryService};

impl UserMemoryService {
    pub fn with_managed_chat_root(mut self, data_dir: &Path) -> Self {
        self.managed_chat_root = Some(crate::commands::skill_inventory::workspace_key(Some(
            data_dir.join("chat-sessions").to_string_lossy().as_ref(),
        )));
        self
    }

    pub(super) fn memory_workspace_key(&self, workspace: Option<&str>) -> Option<String> {
        let workspace = workspace.filter(|value| !value.is_empty())?;
        let Some(root) = self.managed_chat_root.as_deref() else {
            return Some(workspace.to_string());
        };
        if is_managed_chat_workspace(root, workspace) {
            return Some(format!("managed-chat:{root}"));
        }
        Some(workspace.to_string())
    }

    pub(super) fn memory_recall_scope(
        &self,
        scope: UserMemoryRecallScope,
    ) -> UserMemoryRecallScope {
        match scope {
            UserMemoryRecallScope::Workspace(key) => UserMemoryRecallScope::from_workspace_key(
                self.memory_workspace_key(Some(&key)).unwrap_or_default(),
            ),
            scope => scope,
        }
    }

    pub(super) fn scope_index_snapshot(&self, mut snapshot: IndexSnapshot) -> IndexSnapshot {
        for item in &mut snapshot.items {
            if item.kind == "experience" && item.scope_type == "workspace" {
                if let Some(key) = self.memory_workspace_key(Some(&item.scope_key)) {
                    item.scope_key = key;
                }
            }
        }
        snapshot.source_digest = self.scoped_source_digest(&snapshot.source_digest);
        snapshot
    }

    pub(super) fn scoped_source_digest(&self, source: &str) -> String {
        hash_parts(&[
            b"memory-scope-v1",
            source.as_bytes(),
            self.managed_chat_root
                .as_deref()
                .unwrap_or_default()
                .as_bytes(),
        ])
    }
}

fn is_managed_chat_workspace(root: &str, workspace: &str) -> bool {
    let Some(relative) = workspace
        .strip_prefix(root)
        .and_then(|path| path.strip_prefix('/'))
    else {
        return false;
    };
    let mut segments = relative.split('/');
    let (Some(date), Some(id), None) = (segments.next(), segments.next(), segments.next()) else {
        return false;
    };
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok() && uuid::Uuid::parse_str(id).is_ok()
}
