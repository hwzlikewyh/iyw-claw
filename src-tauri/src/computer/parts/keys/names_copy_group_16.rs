// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether a menu command is named for copying or cutting — whose change of
/// the clipboard is then watched, as a copying key's is.
pub fn names_copy(title: &str) -> bool {
    let title = title.to_lowercase();
    COPY_WORDS.iter().any(|word| title.contains(word))
}
