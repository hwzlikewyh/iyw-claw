// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Which application [`TargetTable::share_app`] is to share: the one a window
/// is of, or one already shared, by its share's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTarget<'a> {
    Window(&'a str),
    Share(&'a str),
}
