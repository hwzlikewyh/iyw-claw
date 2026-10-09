// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Windows: the window classes of the shell's views of other windows — Task
/// View and the window switcher (Windows 10's, and 11's), snapping's
/// suggestions, and the taskbar's previews.
#[cfg(windows)]
pub(super) const OVERVIEW_CLASSES: &[&str] = &[
    "MultitaskingViewFrame",
    "XamlExplorerHostIslandWindow",
    "TaskListThumbnailWnd",
];

/// Windows: whether a window of class `class` shows what other windows hold
/// ([`OVERVIEW_CLASSES`]).
#[cfg(windows)]
pub(super) fn shows_others(class: &str) -> bool {
    OVERVIEW_CLASSES.contains(&class)
}
