// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PointerButton {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScrollUnit {
    #[default]
    Line,
    Page,
}

/// The most text one `computer_type` or `computer_set_value` carries.
pub const MAX_ACTION_TEXT_CHARS: usize = 10_000;

/// The most times one `computer_press_key` presses its key.
pub const MAX_KEY_REPEAT: u32 = 20;

/// The most wheel notches (or keystrokes) one `computer_scroll` sends.
pub const MAX_SCROLL_AMOUNT: u32 = 25;

/// The longest a drag's path may take, the driver's own bound.
pub const MAX_DRAG_MS: u32 = 10_000;

/// The longest one `computer_hold_key` holds its key. Each press takes its
/// own turn at the driver, so other calls go in between; the bound is on how
/// long one call keeps pressing.
pub const MAX_HOLD_MS: u32 = 10_000;
