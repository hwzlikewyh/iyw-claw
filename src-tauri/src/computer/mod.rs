//! Computer use: letting an agent look at the native windows on the user's
//! desktop — one window at a time, and only the ones a person shared.
//!
//! Acting on a shared window follows the same rules, one level up: a window
//! shared for control, one element or one point of what the agent last read
//! of it, keys that stay inside the window, delivered in the background
//! unless the person lets agents bring a window to the front for an action,
//! and a Stop the person can press at any moment.
//!
//! 桌面执行器与主程序共用文件，在后台子进程中执行截图和输入。
//! macOS 的辅助功能与屏幕录制授权属于主应用，其子进程可能沿用权限。
//! 窗口共享和停止仅约束本应用的电脑工具，不声称隔离智能体 shell。
//! 签名包保留调用方校验，cua-driver 保持固定摘要及映像验证。
//!
//! Module map:
//! - `types`     — wire types shared with the companion and the frontend
//! - `agent`     — grant rules: what may be shared, what a grant covers, how
//!   titles are narrowed, when a grant lapses
//! - `keys`      — the keys an agent may press, and which a window grant
//!   allows
//! - `stop_shortcut` — the global shortcut that stops every agent at once
//! - `targets`   — iyw-claw's table of windows it has told an agent about, with
//!   the grant on each entry
//! - `protocol`  — frames between iyw-claw and the helper
//! - `driver`    — the pinned cua-driver release and its trust anchors
//! - `backend`   — the trait the tool surface calls, and its errors
//! - `codesign`  — macOS code-signature checks (Security.framework)
//! - `launch_req` — macOS launch requirements: the kernel's check of what a
//!   spawn may run
//! - `spawn`     — macOS `posix_spawn` with the attributes the design needs
//! - `tcc`       — macOS read-only TCC preflight queries
//! - `procinfo`  — process start times, so a reused pid is not the same app
//! - `appident`  — which application a process is, read off the process; a
//!   frame on Windows is the one drawing inside it
//! - `helper`    — the helper process's own logic (runs in the helper binary)
//! - `helper_app` — 独立服务端执行器的兼容 app 布局，
//!   桌面端不再使用此副本
//! - `local`     — iyw-claw's side of the helper: launch, verify, talk
//! - `events`    — what the frontend is told
//! - `driver_admin` — the driver as Settings manages it: install, clear, remove
//! - `stop_key`  — the stop shortcut as the OS holds it
//! - `indicator` — the strip above every window while anything is shared
//! - `marker`    — the mark an action leaves where it landed

pub mod agent;
pub mod appident;
pub mod backend;
pub mod bootstrap;
pub mod driver;
mod driver_cache;
pub mod entry;
pub mod grants;
pub mod helper;
pub mod keys;
pub mod permission_request;
pub mod procinfo;
pub mod protocol;
pub mod stop_shortcut;
pub mod targets;
pub(crate) mod tool_arguments;
pub(crate) mod tool_dispatch;
pub(crate) mod tool_render;
pub mod types;

#[cfg(target_os = "macos")]
pub mod codesign;
#[cfg(target_os = "macos")]
pub mod launch_req;
#[cfg(target_os = "macos")]
pub mod spawn;
#[cfg(target_os = "macos")]
pub mod tcc;

// iyw-claw's side of the helper and the events it raises: the desktop app's,
// and iyw-claw-server's where the person who runs it lets it share the screen
// it runs on (`IYW_CLAW_COMPUTER_USE`).
pub mod driver_admin;
pub mod events;
#[cfg(target_os = "macos")]
pub mod helper_app;
pub mod local;

// What only the desktop app has: a window above every other, the mark an
// action leaves, and a shortcut held with the OS.
#[cfg(feature = "tauri-runtime")]
pub mod indicator;
#[cfg(feature = "tauri-runtime")]
pub mod marker;
#[cfg(feature = "tauri-runtime")]
pub mod stop_key;
