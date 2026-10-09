//! Windows: what the system says of a window the driver listed, asked by its
//! handle — which is what the driver's window id is there.
//!
//! The driver's listing leaves two things unsaid that decide whether a window
//! is one a person could mean to share, and whose it is:
//!
//! - **Whether it is drawn.** Windows can hide a window while leaving it
//!   visible by every other measure: the compositor *cloaks* it. The windows
//!   of the other virtual desktops are cloaked, and so are the ones the system
//!   keeps ready out of sight — the input experience (the emoji panel, the
//!   touch keyboard), a packaged application's own window while its frame is
//!   minimized. A cloaked window is off the screen: on another desktop, it is
//!   what a window on another Space is on macOS; on this one, furniture
//!   nobody can see. Only a window the system places on this desktop is taken
//!   for furniture: one it will not place stays as the driver listed it.
//! - **What a frame shows.** A packaged application draws inside the frame
//!   `ApplicationFrameHost` draws for it, in a core window of its own process
//!   set into the frame (see `appident`). Which process that is, is read off
//!   the core window: its owner, which the system keeps and no process can
//!   say otherwise of. While the frame is minimized, the core window stands
//!   outside it, cloaked, and the frame says which application it shows (by
//!   its application user model id): the one process running that
//!   application with a core window standing on its own is the one. None, or
//!   more than one, and the frame is nobody's that can be told.

use std::ffi::c_void;
use std::ptr;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{BOOL, HWND, RECT};

use crate::computer::appident::Com;
use crate::computer::procinfo::{process_image, process_start_while};
use crate::computer::protocol::ProcessRun;
use crate::computer::types::Rect;

#[path = "parts/hwnd/CORE_WINDOW_CLASS_group_1.rs"]
mod part_1;
use part_1::*;

// Declared here: windows-sys has these behind features this crate does not
// turn on (`Win32_UI_WindowsAndMessaging`, `Win32_Graphics_Dwm`,
// `Win32_System_Com`, `Win32_UI_Shell_PropertiesSystem`), and turning one on
// rebuilds every crate that shares windows-sys — Tauri among them.
#[link(name = "user32")]
extern "system" {
    fn GetWindowThreadProcessId(window: HWND, pid: *mut u32) -> u32;
    fn FindWindowExW(parent: HWND, after: HWND, class: *const u16, title: *const u16) -> HWND;
    fn IsIconic(window: HWND) -> BOOL;
    fn ShowWindowAsync(window: HWND, command: i32) -> BOOL;
    fn EnumWindows(callback: unsafe extern "system" fn(HWND, isize) -> BOOL, param: isize) -> BOOL;
    fn IsWindowVisible(window: HWND) -> BOOL;
    fn GetWindowLongW(window: HWND, index: i32) -> i32;
    fn GetClassNameW(window: HWND, name: *mut u16, capacity: i32) -> i32;
}

#[path = "parts/hwnd/GWL_EXSTYLE_group_2.rs"]
mod part_2;
use part_2::*;

#[link(name = "dwmapi")]
extern "system" {
    fn DwmGetWindowAttribute(window: HWND, attribute: u32, value: *mut c_void, size: u32) -> i32;
}
#[link(name = "ole32")]
extern "system" {
    fn CoCreateInstance(
        class: *const GUID,
        outer: *mut c_void,
        context: u32,
        interface: *const GUID,
        object: *mut *mut c_void,
    ) -> i32;
    fn PropVariantClear(value: *mut PropVariant) -> i32;
}
#[link(name = "shell32")]
extern "system" {
    fn SHGetPropertyStoreForWindow(
        window: HWND,
        interface: *const GUID,
        store: *mut *mut c_void,
    ) -> i32;
}

#[path = "parts/hwnd/PropertyKey_group_3.rs"]
mod part_3;
use part_3::*;

#[path = "parts/hwnd/empty_group_4.rs"]
mod part_4;

#[path = "parts/hwnd/UnknownMethods_group_5.rs"]
mod part_5;
use part_5::*;

#[path = "parts/hwnd/methods_group_6.rs"]
mod part_6;

#[path = "parts/hwnd/Desktop_group_7.rs"]
mod part_7;
pub use part_7::*;

#[path = "parts/hwnd/open_group_8.rs"]
mod part_8;

#[path = "parts/hwnd/frame_holds_group_9.rs"]
mod part_9;
pub use part_9::*;

#[path = "parts/hwnd/class_of_group_10.rs"]
mod part_10;
use part_10::*;
