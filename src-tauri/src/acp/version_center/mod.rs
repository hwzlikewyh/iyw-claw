//! Trusted client-side boundary for the Agent and managed-tool version center.
//!
//! The remote service selects immutable versions and signs download tickets.
//! It never defines a command, package identity, executable layout, or PATH.

mod capability;
mod catalog;
mod client;
pub(crate) mod fallback;
mod installer;
mod inventory;
mod launch_authorization;
mod npm_install;
mod official_sources;
mod runtime_bundle_archive;
mod runtime_bundle_manifest;
mod runtime_bundle_state;
mod types;
mod uvx_install;

pub use capability::{current_arch, current_target, known_tool, RUNTIME, TOOL_IDS};
pub(crate) use catalog::platform_id;
pub use catalog::{
    platform_projection, CatalogStore, CatalogView, PlatformAccess, PlatformProjection,
};
pub use client::{AgentPlatformClient, CapabilityPolicyHttpFetcher, CatalogFetch};
// Task 06 新增统一初始化入口的再导出（最小改动：仅追加三行，供命令层/前端接线）。
pub use installer::{
    bootstrap_init_status, bootstrap_initialize, consume_pending_activations_at_startup,
    digest_managed_root, install_managed_tool, managed_tool_executable, InitStatusReport,
    ManagedToolInstallResult,
};
pub(crate) use installer::{
    extract_tool_zip, install_managed_binary_agent, locate_payload,
    pending_agent_activation_version, persisted_activation_revision, runtime_dir,
    write_current_pointer, ManagedBinaryAgentRequest,
};
pub(crate) use installer::{
    install_runtime_bundle, InstalledRuntimeBundle, RuntimeBundleInstallError, RuntimeBundleRequest,
};
pub(crate) use installer::{push_pending_activation, PendingActivation};
pub use inventory::{
    activate_agent, list_agent_installations, list_tool_installations, list_tool_settings,
    promote_agent_lkg, record_agent_ready, recover_agent, set_agent_pin, set_tool_pin,
    AgentInstallation, ManagedToolInstallation, ManagedToolSetting, ReadyAgentInstallation,
};
pub(crate) use launch_authorization::authorize_agent_version_launch;
pub(crate) use npm_install::{
    confirm_npm_agent_install, ensure_npm_node_requirement, resolve_npm_agent_install,
    ManagedNpmInstall,
};
pub(crate) use official_sources::{fallback_npm_agent_install, fallback_uvx_agent_install};
pub(crate) use runtime_bundle_state::{remove_uvx_bundles, uvx_bundle_env};
pub(crate) use types::ResolveAgentRequest;
pub use types::{AgentOffer, CatalogSnapshot, DownloadTicket, ToolOffer, VersionHistory};
pub(crate) use uvx_install::{confirm_uvx_agent_install, resolve_uvx_agent_install};
