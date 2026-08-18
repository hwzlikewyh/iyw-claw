use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    AgentLaunch,
    HostExecution,
    HostRead,
    HostWrite,
    Terminal,
    Mcp,
    FileUpload,
    ProjectBoot,
    FolderLinks,
    SplitView,
    WorkTasks,
    WorkTaskMerge,
}

impl Capability {
    pub fn compiled_support(self) -> bool {
        matches!(
            self,
            Self::AgentLaunch
                | Self::HostExecution
                | Self::HostRead
                | Self::HostWrite
                | Self::Terminal
                | Self::Mcp
                | Self::FileUpload
        )
    }

    pub fn is_agent_scoped(self) -> bool {
        matches!(
            self,
            Self::AgentLaunch
                | Self::HostExecution
                | Self::HostRead
                | Self::HostWrite
                | Self::Terminal
                | Self::Mcp
        )
    }

    pub fn requires_host_execution(self) -> bool {
        matches!(self, Self::HostRead | Self::HostWrite | Self::Terminal)
    }

    pub(super) fn is_sensitive(self) -> bool {
        self != Self::AgentLaunch
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::AgentLaunch => "agent_launch",
            Self::HostExecution => "host_execution",
            Self::HostRead => "host_read",
            Self::HostWrite => "host_write",
            Self::Terminal => "terminal",
            Self::Mcp => "mcp",
            Self::FileUpload => "file_upload",
            Self::ProjectBoot => "project_boot",
            Self::FolderLinks => "folder_links",
            Self::SplitView => "split_view",
            Self::WorkTasks => "work_tasks",
            Self::WorkTaskMerge => "work_task_merge",
        }
    }
}
