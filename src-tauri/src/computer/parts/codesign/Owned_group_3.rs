// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A Core Foundation object this module owns, released on drop.
pub(super) struct Owned(CFTypeRef);
