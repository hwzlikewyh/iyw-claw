// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Which desktop the rules are being applied for. The chords differ — the
/// shortcut modifier is ⌘ on a Mac and Ctrl elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}
