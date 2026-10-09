//! Wire types for computer use: what an agent is told about the desktop, and
//! what it gets back from a read.
//!
//! camelCase on the wire, like everything the browser tools send, so `targetId`
//! means the same thing in a listing, in a refusal and in the frontend's
//! mirror of these types (`src/lib/computer/types.ts`).
//!
//! Compiled in both runtimes: the main-process HTTP MCP plumbing that carries these is
//! shared code, and server mode has to be able to say "no desktop here" in the
//! same shapes.

use serde::{Deserialize, Serialize};

pub use super::grants::GrantLevel;

#[path = "parts/types/not_shared_group_1.rs"]
mod part_1;
use part_1::*;

#[path = "parts/types/Rect_group_2.rs"]
mod part_2;
pub use part_2::*;

#[path = "parts/types/is_empty_group_3.rs"]
mod part_3;

#[path = "parts/types/AgentAppRef_group_4.rs"]
mod part_4;
pub use part_4::*;

#[path = "parts/types/VerifyOutcome_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/types/generation_group_6.rs"]
mod part_6;

#[path = "parts/types/PointerButton_group_7.rs"]
mod part_7;
pub use part_7::*;

#[path = "parts/types/no_modifiers_group_8.rs"]
mod part_8;
use part_8::*;

#[path = "parts/types/ComputerActRequest_group_9.rs"]
mod part_9;
pub use part_9::*;

#[path = "parts/types/needs_launch_switch_group_10.rs"]
mod part_10;

#[path = "parts/types/ActEffect_group_11.rs"]
mod part_11;
pub use part_11::*;

#[path = "parts/types/as_str_group_12.rs"]
mod part_12;

#[path = "parts/types/ActReport_group_13.rs"]
mod part_13;
pub use part_13::*;
