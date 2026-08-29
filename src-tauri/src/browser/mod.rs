#[cfg(feature = "tauri-runtime")]
mod agent_tool_actions;
#[cfg(feature = "tauri-runtime")]
mod agent_tool_cancellation;
#[cfg(feature = "tauri-runtime")]
mod agent_tool_capture;
#[cfg(feature = "tauri-runtime")]
mod agent_tool_command;
#[cfg(not(feature = "tauri-runtime"))]
mod agent_tool_stub;
#[cfg(feature = "tauri-runtime")]
mod agent_tool_support;
#[cfg(feature = "tauri-runtime")]
mod agent_tools;
#[cfg(feature = "tauri-runtime")]
mod agent_turn_leases;
#[cfg(feature = "tauri-runtime")]
mod agent_window;
#[cfg(feature = "tauri-runtime")]
mod cdp_errors;
#[cfg(feature = "tauri-runtime")]
mod cdp_events;
#[cfg(feature = "tauri-runtime")]
mod cdp_maps;
#[cfg(feature = "tauri-runtime")]
mod cdp_observer;
#[cfg(feature = "tauri-runtime")]
mod cdp_popups;
mod cdp_records;
#[cfg(feature = "tauri-runtime")]
mod command_bootstrap;
#[cfg(feature = "tauri-runtime")]
mod command_output;
#[cfg(feature = "tauri-runtime")]
mod command_runner;
mod control;
mod control_lease;
mod control_waiter;
#[cfg(feature = "tauri-runtime")]
mod engine;
#[cfg(feature = "tauri-runtime")]
mod engine_download;
mod error;
#[cfg(feature = "tauri-runtime")]
mod frame_protocol;
mod manager;
#[cfg(feature = "tauri-runtime")]
mod manager_cdp;
#[cfg(feature = "tauri-runtime")]
mod manager_idle;
#[cfg(feature = "tauri-runtime")]
mod manager_recovery;
#[cfg(feature = "tauri-runtime")]
mod manager_runtime;
#[cfg(feature = "tauri-runtime")]
mod process;
#[cfg(feature = "tauri-runtime")]
mod profile;
#[cfg(feature = "tauri-runtime")]
mod profile_seed;
mod records;
#[cfg(feature = "tauri-runtime")]
mod resource_gate;
#[cfg(feature = "tauri-runtime")]
mod runtime;
#[cfg(feature = "tauri-runtime")]
mod runtime_launch;
#[cfg(feature = "tauri-runtime")]
mod screenshot_quota;
#[cfg(feature = "tauri-runtime")]
mod sidecar;
mod state;
mod state_cdp;
mod state_hosts;
mod state_recovery;
mod state_runtime;
mod state_tabs;
mod state_views;
#[cfg(feature = "tauri-runtime")]
mod stream;
#[cfg(feature = "tauri-runtime")]
mod stream_input;
#[cfg(feature = "tauri-runtime")]
mod stream_lifecycle;
#[cfg(feature = "tauri-runtime")]
mod stream_manager;
#[cfg(feature = "tauri-runtime")]
mod stream_task;
#[cfg(feature = "tauri-runtime")]
mod tab_actions;
#[cfg(feature = "tauri-runtime")]
mod tab_binding;
#[cfg(feature = "tauri-runtime")]
mod tab_cleanup;
#[cfg(feature = "tauri-runtime")]
mod tab_cleanup_registry;
#[cfg(feature = "tauri-runtime")]
mod tab_launch;
#[cfg(feature = "tauri-runtime")]
mod tab_metadata;
#[cfg(feature = "tauri-runtime")]
mod tab_recovery;
#[cfg(feature = "tauri-runtime")]
mod tabs;
mod types;
mod types_cdp;
#[cfg(feature = "tauri-runtime")]
mod user_action;
#[cfg(feature = "tauri-runtime")]
mod user_action_completion;
mod user_control_lease;
#[cfg(feature = "tauri-runtime")]
mod view_host_lifecycle;
#[cfg(feature = "tauri-runtime")]
mod views;
#[cfg(all(feature = "tauri-runtime", target_os = "windows"))]
mod windows_process;
#[cfg(all(feature = "tauri-runtime", target_os = "windows"))]
mod windows_process_values;

pub use control_lease::AgentControlLease;
pub use error::{BrowserError, BrowserErrorCode, BrowserErrorContext};
pub use manager::BrowserSessionManager;
#[cfg(feature = "tauri-runtime")]
pub(crate) use runtime::ManagedBrowserProcessSnapshot;
#[cfg(feature = "tauri-runtime")]
pub use stream_input::*;
pub use types::*;
pub use types_cdp::*;

pub const BROWSER_AGENT_TOOL_NAMES: &[&str] = &[
    "browser_list_tabs",
    "browser_open",
    "browser_read",
    "browser_snapshot",
    "browser_click",
    "browser_fill",
    "browser_press",
    "browser_scroll",
    "browser_wait",
    "browser_screenshot",
    "browser_close_tab",
    "browser_request_user_action",
    "browser_present",
    "browser_close_window",
    "browser_command",
];

pub const MAX_DETACHED_BROWSER_WINDOWS: usize = 8;
