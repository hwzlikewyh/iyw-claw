// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

// The structures below are laid out for the system to read or fill: a field
// the code never names is there for its place.

/// `PROPERTYKEY`.
#[repr(C)]
#[allow(dead_code)]
pub(super) struct PropertyKey {
    pub(in crate::computer::helper::hwnd) format: GUID,
    pub(in crate::computer::helper::hwnd) id: u32,
}

/// `PROPVARIANT`, as far as reading a string out of it goes: its type, and the
/// first pointer-sized word of its value. The system's size and layout, on 32
/// and 64 bits alike.
#[repr(C)]
#[allow(dead_code)]
pub(super) struct PropVariant {
    pub(in crate::computer::helper::hwnd) kind: u16,
    pub(in crate::computer::helper::hwnd) reserved: [u16; 3],
    pub(in crate::computer::helper::hwnd) value: [usize; 2],
}
