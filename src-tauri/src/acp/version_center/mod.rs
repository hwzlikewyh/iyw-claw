//! Trusted client-side boundary for the Agent and managed-tool version center.
//!
//! The remote service selects immutable versions and signs download tickets.
//! It never defines a command, package identity, executable layout, or PATH.

mod capability;
mod catalog;
mod client;
mod installer;
mod inventory;
mod npm_install;
mod types;

pub use capability::{known_tool, TOOL_IDS};
pub use catalog::{CatalogStore, CatalogView};
pub use client::{AgentPlatformClient, CatalogFetch};
// Task 06 新增统一初始化入口的再导出（最小改动：仅追加三行，供命令层/前端接线）。
pub use installer::{
    bootstrap_init_status, bootstrap_initialize, digest_managed_root, install_managed_tool,
    managed_tool_executable, InitStatusReport, ManagedToolInstallResult,
};
pub(crate) use installer::{extract_tool_zip, locate_payload, runtime_dir, write_current_pointer};
pub use inventory::{
    list_agent_installations, list_tool_installations, list_tool_settings, set_agent_pin,
    set_tool_pin, AgentInstallation, ManagedToolInstallation, ManagedToolSetting,
};
pub(crate) use npm_install::resolve_npm_agent_install;
pub use types::{AgentOffer, CatalogSnapshot, DownloadTicket, ToolOffer, VersionHistory};
