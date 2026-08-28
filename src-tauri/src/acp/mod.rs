pub mod account_credentials;
mod account_credentials_formats;
mod agent_image_input;
mod agent_input;
mod agent_input_capabilities;
mod agent_input_control;
mod agent_input_dispatch;
mod agent_input_lifecycle;
mod agent_input_native_turn;
mod agent_input_resume;
mod agent_input_worker;
mod agent_input_worker_dispatch;
mod agent_input_worker_force;
pub mod agent_profile;
pub mod agent_storage;
pub mod agent_storage_work;
mod audio_source;
pub mod audio_transcription;
pub(crate) mod auto_continuation;
pub mod auto_update;
mod automatic_mode;
pub mod automation_tools;
mod channel_confirmation_manager;
pub mod channel_tools;
mod media_tool;
pub use channel_confirmation_manager::ConnectionManagerChannelConfirmationLookup;
pub(crate) mod background_watch;
pub mod binary_cache;
pub(crate) mod builtin_agent_prompt;
pub mod builtin_mcp;
mod builtin_prompt_bridge;
mod builtin_prompt_bridge_file;
mod builtin_prompt_bridge_state;
mod builtin_prompt_injection;
mod builtin_prompt_openclaw;
pub mod capability_policy;
pub mod codex_goal;
pub(crate) mod codex_multi_agent;
pub(crate) mod codex_rollout_migration;
mod codex_rollout_migration_io;
pub mod companion_health;
pub mod connection;
pub(crate) mod connection_tasks;
pub(crate) mod conversation_title_summary;
pub(crate) mod deepseek_config;
mod deepseek_elicitation;
mod deepseek_elicitation_card;
mod deepseek_elicitation_choices;
mod deepseek_elicitation_form;
pub mod delegation;
pub mod error;
pub mod event_stream;
pub mod feedback;
pub mod file_system_runtime;
pub mod fork;
pub mod grok;
pub mod idle_sweep;
pub mod image_analysis;
mod image_analysis_client;
pub mod internal_bus;
pub mod lifecycle;
pub mod manager;
pub mod memory_turn;
pub mod model_catalog;
mod model_catalog_payload;
mod model_catalog_types;
pub(crate) mod model_gateway_chat;
pub mod npm_runtime;
pub mod opencode_catalog;
pub mod opencode_plugins;
pub(crate) mod operation_gate;
pub(crate) mod operation_guard;
mod permission_queue;
mod permission_runtime;
pub(crate) mod plugin_app_events;
pub mod preflight;
pub mod profile_import;
mod profile_import_activation;
mod profile_import_fs;
mod profile_import_io;
mod profile_import_specs;
pub(crate) mod prompt_stall;
pub mod provider_overlay;
mod provider_overlay_files;
mod provider_overlay_formats;
pub mod question;
pub mod registry;
pub(crate) mod resource_governor;
pub mod runtime_context;
pub(crate) mod runtime_host;
mod runtime_host_driver;
pub(crate) mod runtime_host_policy;
mod runtime_host_registry;
mod runtime_host_requests;
mod runtime_host_responses;
mod runtime_host_router;
mod session_config_compat;
pub mod session_config_reconciler;
pub mod session_failure;
pub mod session_info;
mod session_recovery;
pub mod session_state;
pub mod skill_package;
pub mod skill_routing;
pub(crate) mod skill_tree_hash;
pub(crate) mod startup_trace;
pub(crate) mod stderr_tail;
pub(crate) mod task_artifact_delivery;
pub mod terminal_runtime;
pub mod transcript;
pub mod trusted_agents;
pub(crate) mod turn_output;
pub mod types;
pub mod version_center;

pub use auto_update::agent_auto_update_task;
pub use idle_sweep::{
    idle_sweep_task, idle_timeout_from_env, max_idle_connections_from_env,
    prompt_stall_timeout_from_env, SWEEP_INTERVAL_SECS,
};
pub use internal_bus::{EventBusMetrics, EventBusMetricsSnapshot, InternalEventBus};
pub use lifecycle::lifecycle_subscriber_task;
pub use session_state::{LiveSessionSnapshot, SessionState};
// Re-export the inner types of LiveSessionSnapshot for downstream consumers; not all are
// directly named in Rust today (they ride along through the snapshot struct), so silence
// dead-import warnings rather than dropping them.
pub use agent_input::{AgentInputItem, AgentInputPayload, AgentInputStatus, AgentInputStrategy};
#[allow(unused_imports)]
pub use session_state::{
    LiveContentBlock, LiveMessage, PendingPermissionState, ToolCallOutput, ToolCallState,
    ToolCallStatus, ToolKind, UsageInfo,
};
pub use types::{
    user_blocks_from_prompt, AcpEvent, ConversationConnectionInfo, EventEnvelope, UserMessageBlock,
};

pub fn cleanup_stale_builtin_prompt_bridges(
    paths: &agent_storage::AgentStoragePaths,
) -> Result<(), error::AcpError> {
    builtin_prompt_bridge::cleanup_stale(paths)
}
