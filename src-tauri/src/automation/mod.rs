//! Automation execution engine (distinct from `commands::automation`, which is
//! the CRUD surface). One engine per process, built at boot in both desktop and
//! server mode; see [`engine`].

pub mod default_folder;
pub mod default_mode;
pub mod draft;
pub mod engine;
#[cfg(feature = "tauri-runtime")]
pub(crate) mod keep_awake;
pub mod project_skill;
pub mod notifications;
mod notification_message;
pub mod retry_queue;

pub use engine::{build_engine, engine, run_automation_engine, AutomationEngine};
