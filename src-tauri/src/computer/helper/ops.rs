//! Each read op, as exactly one driver call and a translation of its answer.
//!
//! This, with [`super::act`] for the ops that change a window, is the
//! whitelist. The driver advertises several dozen tools; the ones named here
//! (`list_apps`, `list_windows`, `get_window_state`, `verify_state`) and
//! there are the only ones the helper ever calls, with arguments built from
//! typed fields — never a tool name or an argument object that came from
//! iyw-claw as-is. (What the driver's listing leaves unsaid — which windows are
//! minimized, and on macOS which applications hidden — the helper asks the
//! system itself: see `mark_out_of_sight`.)

use std::collections::HashMap;
use std::time::Duration;

use serde_json::{json, Map, Value};

use super::act::{ElementFacts, SnapshotFacts};
use super::driver_proc::DriverProc;
use super::mcp::ToolCallResult;
use super::tree::{
    is_masked, names_a_secret, redact_secrets, without_app_menus, AppMenus, Dialect, TreeNode,
    APP_MENU_ROLES,
};
use crate::computer::procinfo::process_start;
use crate::computer::protocol::{
    HelperError, HelperErrorCode, InstalledApp, OsPermission, RawApp, RawCapture, RawClipboard,
    RawLaunch, RawSnapshot, RawVerify, RawWindow, SnapshotRef,
};
use crate::computer::types::{
    PredicateResult, Rect, VerifyPredicate, VerifyRequest, VerifyStatus, MAX_VERIFY_PREDICATES,
};

#[path = "parts/ops/LIST_TIMEOUT_group_1.rs"]
mod part_1;
use part_1::*;

#[path = "parts/ops/tool_error_group_2.rs"]
mod part_2;
pub use part_2::*;

#[path = "parts/ops/call_group_3.rs"]
mod part_3;
use part_3::*;

#[path = "parts/ops/structured_group_4.rs"]
mod part_4;
pub(super) use part_4::*;

#[path = "parts/ops/string_group_5.rs"]
mod part_5;
use part_5::*;

#[cfg(not(any(target_os = "macos", windows)))]
#[path = "parts/ops/list_apps_group_6.rs"]
mod part_6;
#[cfg(not(any(target_os = "macos", windows)))]
pub use part_6::*;

#[cfg(any(test, target_os = "macos", windows))]
#[path = "parts/ops/apps_of_group_7.rs"]
mod part_7;
#[cfg(any(test, target_os = "macos", windows))]
pub use part_7::*;

#[path = "parts/ops/LAUNCH_TIMEOUT_group_8.rs"]
mod part_8;
use part_8::*;

#[path = "parts/ops/find_app_group_9.rs"]
mod part_9;
pub use part_9::*;

#[path = "parts/ops/MAX_CLIPBOARD_CHARS_group_10.rs"]
mod part_10;
use part_10::*;

#[path = "parts/ops/clipboard_read_group_11.rs"]
mod part_11;
pub use part_11::*;

#[path = "parts/ops/required_array_group_12.rs"]
mod part_12;
use part_12::*;

#[cfg(any(test, not(any(target_os = "macos", windows))))]
#[path = "parts/ops/parse_apps_group_13.rs"]
mod part_13;
#[cfg(any(test, not(any(target_os = "macos", windows))))]
use part_13::*;

#[path = "parts/ops/AppCache_group_14.rs"]
mod part_14;
pub use part_14::*;

#[path = "parts/ops/lookup_group_15.rs"]
mod part_15;

#[path = "parts/ops/list_windows_group_16.rs"]
mod part_16;
pub use part_16::*;

#[cfg(not(any(target_os = "macos", windows)))]
#[path = "parts/ops/driver_apps_group_17.rs"]
mod part_17;
#[cfg(not(any(target_os = "macos", windows)))]
use part_17::*;

#[cfg(target_os = "macos")]
#[path = "parts/ops/join_identified_group_18.rs"]
mod part_18;
#[cfg(target_os = "macos")]
use part_18::*;

#[cfg(target_os = "macos")]
#[path = "parts/ops/identified_group_19.rs"]
mod part_19;
#[cfg(target_os = "macos")]
pub(super) use part_19::*;

#[cfg(windows)]
#[path = "parts/ops/join_identified_group_20.rs"]
mod part_20;
#[cfg(windows)]
pub(super) use part_20::*;

#[path = "parts/ops/parse_windows_group_21.rs"]
mod part_21;
pub(super) use part_21::*;

#[cfg(target_os = "macos")]
#[path = "parts/ops/mark_out_of_sight_group_22.rs"]
mod part_22;
#[cfg(target_os = "macos")]
pub use part_22::*;

#[cfg(all(target_os = "linux", feature = "computer-executor"))]
#[path = "parts/ops/mark_out_of_sight_group_23.rs"]
mod part_23;
#[cfg(all(target_os = "linux", feature = "computer-executor"))]
pub use part_23::*;

#[cfg(any(test, target_os = "macos"))]
#[path = "parts/ops/AppWindows_group_24.rs"]
mod part_24;
#[cfg(any(test, target_os = "macos"))]
pub use part_24::*;

#[cfg(any(test, target_os = "macos"))]
#[path = "parts/ops/unexplained_group_25.rs"]
mod part_25;
#[cfg(any(test, target_os = "macos"))]
use part_25::*;

#[cfg(target_os = "macos")]
#[path = "parts/ops/out_of_sight_capture_group_26.rs"]
mod part_26;
#[cfg(target_os = "macos")]
pub use part_26::*;

#[path = "parts/ops/capture_group_27.rs"]
mod part_27;
pub use part_27::*;

#[path = "parts/ops/Taken_group_28.rs"]
mod part_28;
use part_28::*;

#[cfg(not(target_os = "linux"))]
#[path = "parts/ops/take_capture_group_29.rs"]
mod part_29;
#[cfg(not(target_os = "linux"))]
use part_29::*;

#[cfg(any(not(target_os = "linux"), test))]
#[path = "parts/ops/same_size_group_30.rs"]
mod part_30;
#[cfg(any(not(target_os = "linux"), test))]
use part_30::*;

#[cfg(target_os = "linux")]
#[path = "parts/ops/take_capture_group_31.rs"]
mod part_31;
#[cfg(target_os = "linux")]
use part_31::*;

#[path = "parts/ops/reckoned_scale_group_32.rs"]
mod part_32;
use part_32::*;

#[path = "parts/ops/Shrunk_group_33.rs"]
mod part_33;
pub(super) use part_33::*;

#[path = "parts/ops/is_whole_window_group_34.rs"]
mod part_34;
use part_34::*;

#[path = "parts/ops/snapshot_group_35.rs"]
mod part_35;
pub use part_35::*;

#[cfg(target_os = "macos")]
#[path = "parts/ops/protected_menu_titles_group_36.rs"]
mod part_36;
#[cfg(target_os = "macos")]
use part_36::*;

#[cfg(not(target_os = "macos"))]
#[path = "parts/ops/protected_menu_titles_group_37.rs"]
mod part_37;
#[cfg(not(target_os = "macos"))]
use part_37::*;

#[path = "parts/ops/element_refs_group_38.rs"]
mod part_38;
use part_38::*;

#[path = "parts/ops/driver_predicates_group_39.rs"]
mod part_39;
pub use part_39::*;

#[path = "parts/ops/verify_status_group_40.rs"]
mod part_40;
use part_40::*;

#[path = "parts/ops/verify_group_41.rs"]
mod part_41;
pub use part_41::*;

#[path = "parts/ops/parse_verify_group_42.rs"]
mod part_42;
use part_42::*;
