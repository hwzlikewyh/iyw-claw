// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The punctuation keys of the main block, as their unshifted characters.
pub(super) const PUNCTUATION: &[char] = &['-', '=', '[', ']', '\\', ';', '\'', ',', '.', '/', '`'];
