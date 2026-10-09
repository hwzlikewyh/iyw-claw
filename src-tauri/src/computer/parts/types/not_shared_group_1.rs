// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) fn not_shared(level: &GrantLevel) -> bool {
    *level == GrantLevel::None
}
