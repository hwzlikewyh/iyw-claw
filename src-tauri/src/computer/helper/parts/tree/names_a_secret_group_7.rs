// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether `text` — a node's role and label, never its value — says it is a
/// password or other secret field.
pub fn names_a_secret(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("securetextfield")
        || lower.contains("password text")
        || SECRET_WORDS.iter().any(|w| lower.contains(w))
}

/// A value that is nothing but the bullets a secure field shows in place of
/// its text.
pub fn is_masked(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.chars().all(|c| matches!(c, '•' | '●' | '∙' | '⦁'))
}
