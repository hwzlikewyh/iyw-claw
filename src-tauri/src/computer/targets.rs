//! iyw-claw's table of the windows it has named to an agent, with the grant on
//! each.
//!
//! A browser tab is an object iyw-claw owns, so its grant can live on the tab. A
//! native window is not — it belongs to another process and iyw-claw only ever
//! sees it through the helper — so this table is the object the grant lives
//! on: one entry per window iyw-claw has handed out a `targetId` for, keyed by
//! the window's identity, with the grant, the grant epoch and the read counter
//! under the same lock. "Is this still the window that was shared" and "is it
//! still shared" are then one question asked in one place.
//!
//! **Identity is `(pid, process start time, window id)`.** A pid alone is
//! reused; an application relaunched is a new process whose windows were never
//! shared, even when they look the same. A window whose identity no longer
//! turns up in a listing is gone, and so is its grant. Where another process
//! draws what is inside a window — a packaged application inside the frame
//! Windows draws for it — that process's run is part of the identity too:
//! the frame is that run's window, and another run of it in the same frame is
//! another window.
//!
//! **An application can be shared as a whole** ([`AppIdentity`]): every window
//! of it then carries the application's grant ([`GrantScope::App`]) — the
//! ones it opens later too, as listings find them — and the application's
//! one clock, which any of them being used keeps running. Ending the
//! application's grant ends every window's share of it; a window's share
//! cannot be changed on its own while the application is shared.
//!
//! **So can the entire screen** ([`ScreenShare`]), the same way one level up:
//! every window the rules allow carries the screen's grant
//! ([`GrantScope::Screen`]) and its clock, and the screen itself is a target
//! of its own ([`SCREEN_TARGET_ID`]) — one picture of it, and pointer actions
//! at points on it. Sharing the screen takes over whatever was shared before
//! it; nothing else is shared or unshared while it is.

use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::agent::{
    generation, grantable, level_of, visible_title, Blocklist, ComputerGrant, ComputerGrantPayload,
    GrantChange, GrantLevel, GrantScope, NotGrantable, SelfIdentity,
};
use super::keys::{classify, classify_for_app, classify_for_screen, Chord, ChordClass, Platform};
use super::protocol::{
    DriverTarget, ElementRef, ProcessRun, RawAct, RawApp, RawWindow, ScreenGeometry, WindowAction,
    WindowPoint,
};
use super::types::{
    AgentAppRef, AgentTarget, AgentWindowSummary, ComputerActRequest, ElementTarget, PointTarget,
    Rect,
};

#[path = "parts/targets/WindowIdentity_group_1.rs"]
mod part_1;
pub use part_1::*;

#[path = "parts/targets/of_group_2.rs"]
mod part_2;

#[path = "parts/targets/TargetEntry_group_3.rs"]
mod part_3;
pub use part_3::*;

#[path = "parts/targets/worth_listing_group_4.rs"]
mod part_4;

#[path = "parts/targets/SharedWindow_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/targets/of_group_6.rs"]
mod part_6;

#[path = "parts/targets/AppShare_group_7.rs"]
mod part_7;
pub use part_7::*;

#[path = "parts/targets/ActTicket_group_8.rs"]
mod part_8;
pub use part_8::*;

#[path = "parts/targets/of_group_9.rs"]
mod part_9;

#[path = "parts/targets/ShareError_group_10.rs"]
mod part_10;
pub use part_10::*;

#[path = "parts/targets/Inner_group_11.rs"]
mod part_11;
use part_11::*;

#[path = "parts/targets/TargetTable_group_12.rs"]
mod part_12;
pub use part_12::*;

#[path = "parts/targets/new_group_13.rs"]
mod part_13;
#[path = "parts/targets/share_app_group_14.rs"]
mod part_14;
#[path = "parts/targets/end_grant_group_15.rs"]
mod part_15;
#[path = "parts/targets/begin_read_group_16.rs"]
mod part_16;
#[path = "parts/targets/shared_group_17.rs"]
mod part_17;

#[path = "parts/targets/AppTarget_group_18.rs"]
mod part_18;
pub use part_18::*;

#[path = "parts/targets/absorb_group_19.rs"]
mod part_19;

#[path = "parts/targets/resolve_group_20.rs"]
mod part_20;
use part_20::*;

#[path = "parts/targets/check_chord_group_21.rs"]
mod part_21;
use part_21::*;
