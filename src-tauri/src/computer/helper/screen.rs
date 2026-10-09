//! The entire screen, shared as a whole: one picture of it, and pointer
//! actions at points on it.
//!
//! Sharing the screen does not lift the rules that hold for any window: iyw-claw
//! itself and the applications on the blocklist are never seen nor touched.
//! Over the whole screen the helper judges that itself, window by window, at
//! the moment of the capture or the action, by the rules iyw-claw hands it
//! ([`ScreenRules`]) — so a window that came up since iyw-claw last looked is
//! judged too, not let through for want of having been listed. A window is
//! judged by its owner, as iyw-claw judges a window it lists, and one whose
//! owner cannot be told an application the person could share — the menu
//! bar the system draws, the system's own agents — is judged as one that is
//! never shared. So is a window of the system's that shows what other
//! windows hold — an overview of every window, previews of them, other
//! applications' notifications ([`shows_others`]) — which iyw-claw cannot
//! judge piece by piece; and the keys and corners that bring such an
//! overview up are kept out of reach (`keys::classify_for_screen`,
//! [`in_a_corner`]).
//!
//! Such a window is painted over in the picture, whatever is in front of it
//! and whatever layer it is on (a password manager's panel floats above the
//! windows an application list shows), with a margin for the edge the
//! pointer takes it by; and a point anywhere on what was painted over is
//! refused. What is refused is then exactly what the agent could not see.
//!
//! The windows come from the system itself, every layer of them and titled
//! or not: on macOS from the window server, on Windows from the window
//! manager's own list — before the picture is taken and again after, and a
//! picture is handed over only when the windows never shared stood still
//! across it: one moving, coming or going while it was taken could have been
//! caught where neither listing has it. A window nothing of shows (macOS:
//! drawn fully transparent) is no window here; one every click passes
//! through (Windows: a layered, click-through overlay) is painted over like
//! any other, and refuses no point — the click lands on what is under it,
//! judged on its own. An action goes only while the screen is as its
//! picture was taken: a point read off a picture of another size or scale
//! would land elsewhere than it was judged. Linux is not offered the screen.

use std::time::Duration;

use serde_json::{json, Value};

use super::driver_proc::DriverProc;
use super::ops::shrink_png;
#[cfg(any(target_os = "macos", windows))]
use crate::computer::agent::grantable;
use crate::computer::agent::Blocklist;
use crate::computer::protocol::{
    HelperError, HelperErrorCode, RawAct, RawCapture, ScreenGeometry, ScreenRules, WindowAction,
    WindowPoint,
};
use crate::computer::types::{ActEffect, PointerButton, Rect, ScrollDirection, ScrollUnit};

#[path = "parts/screen/CAPTURE_TIMEOUT_group_1.rs"]
mod part_1;
use part_1::*;

#[cfg(windows)]
#[path = "parts/screen/EDGE_group_2.rs"]
mod part_2;
#[cfg(windows)]
use part_2::*;

#[cfg(not(windows))]
#[path = "parts/screen/EDGE_group_3.rs"]
mod part_3;
#[cfg(not(windows))]
use part_3::*;

#[path = "parts/screen/ScreenWindow_group_4.rs"]
mod part_4;
pub use part_4::*;

#[path = "parts/screen/reach_group_5.rs"]
mod part_5;

#[path = "parts/screen/windows_group_6.rs"]
mod part_6;
pub use part_6::*;

#[cfg(target_os = "macos")]
#[path = "parts/screen/DOCK_LEVEL_group_7.rs"]
mod part_7;
#[cfg(target_os = "macos")]
use part_7::*;

#[cfg(windows)]
#[path = "parts/screen/OVERVIEW_CLASSES_group_8.rs"]
mod part_8;
#[cfg(windows)]
use part_8::*;

#[cfg(target_os = "macos")]
#[path = "parts/screen/CORNER_group_9.rs"]
mod part_9;
#[cfg(target_os = "macos")]
use part_9::*;

#[path = "parts/screen/in_a_corner_group_10.rs"]
mod part_10;
use part_10::*;

#[cfg(target_os = "macos")]
#[path = "parts/screen/main_display_group_11.rs"]
mod part_11;
#[cfg(target_os = "macos")]
use part_11::*;

#[path = "parts/screen/capture_group_12.rs"]
mod part_12;
pub use part_12::*;

#[path = "parts/screen/Picture_group_13.rs"]
mod part_13;
use part_13::*;

#[cfg(target_os = "macos")]
#[path = "parts/screen/main_display_now_group_14.rs"]
mod part_14;
#[cfg(target_os = "macos")]
use part_14::*;

#[path = "parts/screen/act_group_15.rs"]
mod part_15;
pub use part_15::*;
