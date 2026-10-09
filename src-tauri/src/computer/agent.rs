//! Who may look at a window on an agent's behalf.
//!
//! The rule is the built-in browser's, carried over whole: **nothing is
//! granted automatically.** Not by application, not by which window is in
//! front, not because the agent opened it. The only way an agent reads a
//! window is a person sharing that window. Reading is a grant, not just acting:
//! a screenshot of an unshared window leaks exactly as much as a click on it
//! would do damage, and the windows most worth protecting are the ones a
//! screenshot shows best.
//!
//! The grant lives on the target-table entry for the window
//! (`targets::TargetEntry::grant`), which is why this module is decisions and
//! wire types rather than a store — the same split the browser makes between
//! `browser::agent` and the tab it guards.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use super::grants::GrantLevel;

use super::keys::Platform;
use super::protocol::RawApp;

#[path = "parts/agent/IYW_CLAW_BUNDLE_ID_group_1.rs"]
mod part_1;
pub use part_1::*;

#[path = "parts/agent/new_group_2.rs"]
mod part_2;

#[path = "parts/agent/MAX_CLOCK_SKEW_MS_group_3.rs"]
mod part_3;
pub use part_3::*;

#[path = "parts/agent/note_group_4.rs"]
mod part_4;

#[path = "parts/agent/DefaultBlock_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/agent/names_on_group_6.rs"]
mod part_6;

#[path = "parts/agent/DEFAULT_BLOCKLIST_group_7.rs"]
mod part_7;
pub use part_7::*;

#[path = "parts/agent/new_group_8.rs"]
mod part_8;

#[path = "parts/agent/command_words_group_9.rs"]
mod part_9;
pub use part_9::*;

#[path = "parts/agent/matches_command_group_10.rs"]
mod part_10;

#[path = "parts/agent/SelfIdentity_group_11.rs"]
mod part_11;
pub use part_11::*;

#[path = "parts/agent/current_group_12.rs"]
mod part_12;

#[path = "parts/agent/enclosing_app_bundle_group_13.rs"]
mod part_13;
use part_13::*;

#[path = "parts/agent/grantable_group_14.rs"]
mod part_14;
pub use part_14::*;

#[path = "parts/agent/of_group_15.rs"]
mod part_15;

#[path = "parts/agent/ActivityOutcome_group_16.rs"]
mod part_16;
pub use part_16::*;
