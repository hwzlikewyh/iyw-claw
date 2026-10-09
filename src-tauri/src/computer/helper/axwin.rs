//! What the helper asks of Accessibility itself, rather than of the driver:
//! which windows are minimized, and which applications hidden (⌘H) — and the
//! one change it makes to a window on its own, putting it back on the screen.
//!
//! The driver's window list does not say which windows are minimized, nor
//! whose application is hidden. To it such a window is only off screen — as
//! are the hidden windows every application keeps (a main window closed to
//! the menu bar, a panel made ahead of time), which nobody means to share and
//! iyw-claw leaves out of its lists. The application's accessibility interface
//! tells them apart: it says whether the application is hidden (`AXHidden`),
//! and lists the windows a person can bring up, minimized ones among them,
//! each saying whether it is (`AXMinimized`); ordered-out windows are not in
//! it at all. Nor has the driver a call that restores a window, or shows a
//! hidden application.
//!
//! Only asked once a process started for the purpose has found Accessibility
//! granted to the helper (see `HelperState::permissions`): a process keeps
//! the first "not granted" it hears for the rest of its life, and this one
//! would go on hearing it after the person had said yes. Every question goes
//! to another application and waits for its answer, so each is bounded
//! ([`TIMEOUT`]) and asked off the async runtime.

use std::collections::{BTreeSet, HashMap};

use super::ops::AppWindows;
use std::sync::OnceLock;

use core_foundation::array::CFArray;
use core_foundation::base::{CFGetTypeID, CFType, CFTypeID, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};

#[path = "parts/axwin/AXError_group_1.rs"]
mod part_1;
use part_1::*;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateApplication(pid: libc::pid_t) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> AXError;
    fn AXUIElementGetTypeID() -> CFTypeID;
}

#[path = "parts/axwin/window_states_group_2.rs"]
mod part_2;
pub use part_2::*;

#[path = "parts/axwin/MENU_WALK_group_3.rs"]
mod part_3;
use part_3::*;

#[path = "parts/axwin/protected_menu_titles_group_4.rs"]
mod part_4;
pub use part_4::*;

#[path = "parts/axwin/menu_target_now_group_5.rs"]
mod part_5;
use part_5::*;

#[path = "parts/axwin/Restore_group_6.rs"]
mod part_6;
pub use part_6::*;

#[path = "parts/axwin/restore_now_group_7.rs"]
mod part_7;
use part_7::*;
