//! The ops that change a window, as fixed driver calls.
//!
//! Every action iyw-claw sends is a closed [`WindowAction`]; this module builds
//! the driver's arguments from its fields — the tool (`click`,
//! `double_click`, `right_click`, `scroll`, `type_text`, `press_key`,
//! `set_value`), the window, the element or the point, the `delivery_mode`
//! iyw-claw asked for where the tool takes one (macOS's `set_value` does not) —
//! and nothing else: the driver refuses an argument its tool does not name.
//! That is `"background"` unless iyw-claw asked for the front, which it does
//! only where the person allows it: the driver then brings the window forward
//! for the one call and sends real input — and on macOS and Windows switches
//! back afterwards; on Linux the window stays in front. What the driver would
//! also accept (a desktop scope, a file to write a debug image to, a zoom's
//! coordinates) is never asked for.
//!
//! One action is the helper's own on macOS and Windows: putting a window back
//! on the screen ([`WindowAction::Restore`]), which the driver has no call
//! for that leaves it in the background. On macOS it goes through
//! Accessibility — the application shown again if it is hidden, the window
//! out of the Dock if it is minimized (see [`super::axwin`]); on Windows the
//! window is shown again without being made active (see `super::hwnd`).
//! Either way to that one window of that one process, bringing nothing to the
//! front. On Linux the driver's `bring_to_front` is the only way, and it
//! brings the window to the front: done only when iyw-claw sent the restore
//! for the front. The driver is asked afterwards whether the window is on
//! the screen again.
//!
//! Before a call goes out, what only the helper knows is checked:
//!
//! * **The element.** The driver keeps the latest snapshot of each window and
//!   addresses an element by a token naming that snapshot and the element
//!   (`<snapshot id>:<index>`); the helper keeps the same snapshot
//!   ([`SnapshotBook`]), with what the tree said of each element. A ref from
//!   any other snapshot is stale, and text never goes into an element the tree
//!   judged secret (see [`super::tree`]).
//! * **The point.** A point is in the window's own pixels, read off a
//!   full-size capture of it at a certain size. The window is measured again
//!   now; at another size its contents are laid out elsewhere, and the point
//!   would land on something the agent never saw. The driver aims a point
//!   only for a window whose latest snapshot holds a capture of it: every
//!   snapshot the helper takes captures one (and keeps nothing of it), and
//!   where the driver has none ([`needs_capture`]) the helper takes a
//!   snapshot and sends the action once more — nothing went out the first
//!   time.
//!
//! The rest of "is this input still going where it was meant to" is the
//! driver's own background gate on macOS, which re-reads the window's owner,
//! the element's window and the application's other windows at the moment of
//! delivery, and refuses by code; each code comes back to iyw-claw as one of the
//! helper's, in words written here rather than the driver's.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use serde_json::{json, Value};

use super::driver_proc::DriverProc;
use super::keystate::held_modifiers;
use super::mcp::ToolCallResult;
use super::Delivery;
use crate::computer::keys::{Modifiers, Platform};
use crate::computer::protocol::{
    DriverTarget, ElementRef, HelperError, HelperErrorCode, OsPermission, RawAct, WindowAction,
    WindowPoint,
};
use crate::computer::types::{
    ActDelivery, ActEffect, ActRoute, PointerButton, Rect, ScrollDirection, ScrollUnit,
};

#[path = "parts/act/ACT_TIMEOUT_group_1.rs"]
mod part_1;
use part_1::*;

#[path = "parts/act/ElementFacts_group_2.rs"]
mod part_2;
pub use part_2::*;

#[path = "parts/act/record_group_3.rs"]
mod part_3;

#[path = "parts/act/snapshot_number_group_4.rs"]
mod part_4;
use part_4::*;

#[path = "parts/act/check_points_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/act/listed_group_6.rs"]
mod part_6;
pub(super) use part_6::*;

#[path = "parts/act/restore_group_7.rs"]
mod part_7;
use part_7::*;

#[cfg(target_os = "macos")]
#[path = "parts/act/ask_back_group_8.rs"]
mod part_8;
#[cfg(target_os = "macos")]
use part_8::*;

#[cfg(windows)]
#[path = "parts/act/ask_back_group_9.rs"]
mod part_9;
#[cfg(windows)]
use part_9::*;

#[cfg(not(any(target_os = "macos", windows)))]
#[path = "parts/act/ask_back_group_10.rs"]
mod part_10;
#[cfg(not(any(target_os = "macos", windows)))]
use part_10::*;

#[path = "parts/act/permissions_for_group_11.rs"]
mod part_11;
pub use part_11::*;

#[cfg(target_os = "macos")]
#[path = "parts/act/front_of_its_app_group_12.rs"]
mod part_12;
#[cfg(target_os = "macos")]
use part_12::*;

#[path = "parts/act/pastes_group_13.rs"]
mod part_13;
pub use part_13::*;

#[path = "parts/act/put_modifiers_group_14.rs"]
mod part_14;
use part_14::*;

#[path = "parts/act/needs_capture_group_15.rs"]
mod part_15;
pub use part_15::*;

#[path = "parts/act/HIGHER_RIGHTS_group_16.rs"]
mod part_16;
use part_16::*;

#[path = "parts/act/pointer_actions.rs"]
mod pointer_actions;
use pointer_actions::*;
#[path = "parts/act/keyboard_actions.rs"]
mod keyboard_actions;
use keyboard_actions::*;
#[path = "parts/act/window_actions.rs"]
mod window_actions;
use window_actions::*;

use part_11::wrong_action;

#[path = "parts/act/action_error_notes.rs"]
mod action_error_notes;
use action_error_notes::*;
