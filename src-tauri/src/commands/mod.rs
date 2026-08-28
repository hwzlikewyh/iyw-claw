pub mod acp;
pub mod agent_concurrency;
pub mod agent_input;
pub mod agent_storage;
mod agent_storage_migration;
mod agent_storage_profile;
#[cfg(feature = "tauri-runtime")]
mod agent_storage_tauri;
pub mod agent_version_center;
#[cfg(feature = "tauri-runtime")]
pub mod agent_version_center_tauri;
pub(crate) mod agent_version_operations;
#[cfg(feature = "tauri-runtime")]
pub mod app_update;
pub mod automation;
pub mod automation_draft;
pub mod backup;
#[cfg(feature = "tauri-runtime")]
pub mod browser;
pub mod capability_policy;
#[cfg(feature = "tauri-runtime")]
pub mod capability_policy_tauri;
pub mod chat_attachments;
pub mod chat_channel;
mod chat_channel_delete;
mod chat_channel_token;
pub mod chat_image;
mod chat_image_upload;
pub mod computer_use;
pub mod conversation_context_primer;
mod conversation_history_cache;
mod conversation_history_cache_prune;
pub(crate) mod conversation_title;
pub mod conversations;
pub mod delegation;
#[cfg(feature = "tauri-runtime")]
pub mod desktop;
pub mod display_assets;
pub mod experts;
pub mod feedback;
#[cfg(feature = "tauri-runtime")]
pub mod file_io;
pub mod folder_commands;
pub mod folders;
pub mod idle_agent_settings;
pub mod internet_tools;
pub mod iyw_account;
pub mod iyw_account_profile;
pub mod logging;
pub mod managed_skills;
pub mod mcp;
pub mod mcp_catalog;
mod mcp_catalog_persistence;
pub mod mcp_catalog_sources;
pub mod mcp_sync;
pub mod model_provider;
#[cfg(feature = "tauri-runtime")]
pub mod notification;
pub mod office_tools;
pub mod performance;
pub mod pet;
pub mod plugin_apps;
pub mod question;
pub mod quick_messages;
#[cfg(feature = "tauri-runtime")]
pub mod realtime_voice;
#[cfg(feature = "tauri-runtime")]
pub mod remote_chat_image_upload;
pub mod remote_image;
#[cfg(feature = "tauri-runtime")]
pub mod remote_proxy;
#[cfg(feature = "tauri-runtime")]
pub mod remote_workspace;
pub mod runtime_bootstrap;
pub mod scenarios;
pub mod session_config;
pub mod session_info;
pub mod skill_inventory;
pub mod skill_market;
mod skill_metadata;
pub mod skill_watch;
pub mod system_settings;
pub mod system_skills;
pub mod task_artifacts;
pub mod terminal;
pub mod usage;
pub mod user_memory;
pub mod version_control;
#[cfg(feature = "tauri-runtime")]
pub mod windows;
pub mod workspace_state;
