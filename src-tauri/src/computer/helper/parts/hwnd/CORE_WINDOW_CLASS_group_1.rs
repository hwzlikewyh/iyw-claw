// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The class of the window a packaged application draws in.
pub(super) const CORE_WINDOW_CLASS: &str = "Windows.UI.Core.CoreWindow";

/// The most core windows looked through in one place; there are a handful.
/// A bound, so that windows coming and going under the walk cannot keep it
/// going.
pub(super) const MAX_CORE_WINDOWS: usize = 256;

/// The longest application user model id, in UTF-16 units with its
/// terminating NUL (`APPLICATION_USER_MODEL_ID_MAX_LENGTH`).
pub(super) const MAX_APP_USER_MODEL_ID: usize = 130;

/// `DWMWA_CLOAKED`.
pub(super) const DWMWA_CLOAKED: u32 = 14;

/// `VT_LPWSTR`.
pub(super) const VT_LPWSTR: u16 = 31;

/// `CLSCTX_ALL`.
pub(super) const CLSCTX_ALL: u32 = 0x17;

/// `IID_IPropertyStore`.
pub(super) const IID_PROPERTY_STORE: GUID = GUID::from_u128(0x886d8eeb_8cf2_4446_8d02_cdba1dbdcf99);

/// `PKEY_AppUserModel_ID`.
pub(super) const APP_USER_MODEL_ID_KEY: PropertyKey = PropertyKey {
    format: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
    id: 5,
};

/// `CLSID_VirtualDesktopManager`.
pub(super) const CLSID_VIRTUAL_DESKTOP_MANAGER: GUID =
    GUID::from_u128(0xaa509086_5ca9_4c25_8f95_589d3c07b48a);

/// `IID_IVirtualDesktopManager`.
pub(super) const IID_VIRTUAL_DESKTOP_MANAGER: GUID =
    GUID::from_u128(0xa5cd92ff_29be_454c_8d04_d82879fb3f1b);
