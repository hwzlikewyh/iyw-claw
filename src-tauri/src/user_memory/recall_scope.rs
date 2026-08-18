use sea_orm::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserMemoryRecallScope {
    Global,
    Workspace(String),
}

impl UserMemoryRecallScope {
    pub fn global() -> Self {
        Self::Global
    }

    pub(crate) fn from_workspace_key(key: impl Into<String>) -> Self {
        let key = key.into();
        if key.is_empty() {
            Self::Global
        } else {
            Self::Workspace(key)
        }
    }

    pub(super) fn predicate(&self, table: &str) -> String {
        match self {
            Self::Global => format!("{table}.scope_type = 'global' AND {table}.scope_key = ''"),
            Self::Workspace(_) => format!(
                "(({table}.scope_type = 'global' AND {table}.scope_key = '') OR \
                 ({table}.scope_type = 'workspace' AND {table}.scope_key = ?))"
            ),
        }
    }

    pub(super) fn push_bind(&self, values: &mut Vec<Value>) {
        if let Self::Workspace(key) = self {
            values.push(key.clone().into());
        }
    }

    pub(super) fn permits(&self, scope_type: &str, scope_key: &str) -> bool {
        match self {
            Self::Global => scope_type == "global" && scope_key.is_empty(),
            Self::Workspace(key) => {
                scope_type == "global" && scope_key.is_empty()
                    || scope_type == "workspace" && scope_key == key
            }
        }
    }
}
