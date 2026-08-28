use crate::acp::delegation::companion::CompanionFeatures;

/// Immutable launch-time feature policy shared by MCP discovery and calls.
#[derive(Debug, Clone, Copy)]
pub struct FeatureSnapshot {
    companion: CompanionFeatures,
}

impl FeatureSnapshot {
    pub fn capture(companion: CompanionFeatures) -> Self {
        Self { companion }
    }

    /// The single policy decision used by both `tools/list` and `tools/call`.
    pub fn decision(&self, tool_name: &str) -> ToolDecision {
        if self.companion.allows_tool(tool_name) {
            ToolDecision::Allow
        } else {
            ToolDecision::Deny
        }
    }

    pub fn should_list(&self, tool_name: &str) -> bool {
        self.decision(tool_name).is_allowed()
    }

    pub fn authorize_call(&self, tool_name: &str) -> Result<(), ToolNotAllowed> {
        match self.decision(tool_name) {
            ToolDecision::Allow => Ok(()),
            ToolDecision::Deny => Err(ToolNotAllowed::new(tool_name)),
        }
    }

    pub fn companion_features(&self) -> CompanionFeatures {
        self.companion
    }
}

impl From<CompanionFeatures> for FeatureSnapshot {
    fn from(value: CompanionFeatures) -> Self {
        Self::capture(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolDecision {
    Allow,
    Deny,
}

impl ToolDecision {
    pub fn is_allowed(self) -> bool {
        matches!(self, Self::Allow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("MCP tool `{tool_name}` is not enabled for this session")]
pub struct ToolNotAllowed {
    tool_name: String,
}

impl ToolNotAllowed {
    fn new(tool_name: &str) -> Self {
        Self {
            tool_name: tool_name.to_string(),
        }
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryCapability {
    Append,
    Propose,
    Recall,
    ReadDocuments,
    Management,
}

/// Independent memory permissions captured when the session is issued.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryPermissions {
    append: bool,
    propose: bool,
    recall: bool,
    read_documents: bool,
    management: bool,
}

impl MemoryPermissions {
    pub fn new(
        append: bool,
        propose: bool,
        recall: bool,
        read_documents: bool,
        management: bool,
    ) -> Self {
        Self {
            append,
            propose,
            recall,
            read_documents,
            management,
        }
    }

    pub fn allows(&self, capability: MemoryCapability) -> bool {
        match capability {
            MemoryCapability::Append => self.append,
            MemoryCapability::Propose => self.propose,
            MemoryCapability::Recall => self.recall,
            MemoryCapability::ReadDocuments => self.read_documents,
            MemoryCapability::Management => self.management,
        }
    }

    pub fn append_enabled(&self) -> bool {
        self.append
    }

    pub fn proposal_enabled(&self) -> bool {
        self.propose
    }

    pub fn recall_enabled(&self) -> bool {
        self.recall
    }

    pub fn documents_read_enabled(&self) -> bool {
        self.read_documents
    }

    pub fn management_enabled(&self) -> bool {
        self.management
    }
}
