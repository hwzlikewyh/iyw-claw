//! Automation execution engine (distinct from `commands::automation`, which is
//! the CRUD surface). One engine per process, built at boot in both desktop and
//! server mode; see [`engine`].

pub mod default_folder;
pub mod default_mode;
pub mod draft;
pub mod engine;
pub mod project_skill;

pub use engine::{build_engine, engine, run_automation_engine, AutomationEngine};
