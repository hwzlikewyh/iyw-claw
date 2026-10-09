// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) fn no_modifiers(modifiers: &Modifiers) -> bool {
    modifiers.is_empty()
}
