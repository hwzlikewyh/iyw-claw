// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The start of every COM object's table of methods.
#[repr(C)]
#[allow(dead_code)]
pub(super) struct UnknownMethods {
    pub(in crate::computer::helper::hwnd) query_interface: usize,
    pub(in crate::computer::helper::hwnd) add_ref: usize,
    pub(in crate::computer::helper::hwnd) release:
        unsafe extern "system" fn(this: *mut c_void) -> u32,
}

/// `IPropertyStore`'s table, as far as `GetValue`.
#[repr(C)]
#[allow(dead_code)]
pub(super) struct PropertyStoreMethods {
    pub(in crate::computer::helper::hwnd) unknown: UnknownMethods,
    pub(in crate::computer::helper::hwnd) get_count: usize,
    pub(in crate::computer::helper::hwnd) get_at: usize,
    pub(in crate::computer::helper::hwnd) get_value: unsafe extern "system" fn(
        this: *mut c_void,
        key: *const PropertyKey,
        value: *mut PropVariant,
    ) -> i32,
}

/// `IVirtualDesktopManager`'s table, as far as
/// `IsWindowOnCurrentVirtualDesktop`.
#[repr(C)]
#[allow(dead_code)]
pub(super) struct VirtualDesktopManagerMethods {
    pub(in crate::computer::helper::hwnd) unknown: UnknownMethods,
    pub(in crate::computer::helper::hwnd) is_window_on_current_virtual_desktop:
        unsafe extern "system" fn(this: *mut c_void, window: HWND, on: *mut BOOL) -> i32,
}

/// A COM object we hold a reference to, released when dropped.
pub(super) struct Object(pub(in crate::computer::helper::hwnd) *mut c_void);
