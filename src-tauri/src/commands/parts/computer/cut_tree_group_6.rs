// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Cut `tree` to at most `max_chars` characters, on a line boundary. `0` is
/// no cap. Returns the tree and whether anything was cut.
pub(super) fn cut_tree(tree: &str, max_chars: usize) -> (String, bool) {
    if max_chars == 0 || tree.chars().count() <= max_chars {
        return (tree.to_string(), false);
    }
    let mut out = String::new();
    let mut used = 0usize;
    for line in tree.split_inclusive('\n') {
        let len = line.chars().count();
        if used + len > max_chars {
            break;
        }
        out.push_str(line);
        used += len;
    }
    (out, true)
}

/// A refusal before or during a read, as the slug and the words.
pub(super) struct Refusal {
    pub(in crate::commands::computer) slug: &'static str,
    pub(in crate::commands::computer) note: String,
    /// What the activity line records.
    pub(in crate::commands::computer) outcome: ActivityOutcome,
    /// An action that failed after it was sent: it may have happened.
    pub(in crate::commands::computer) maybe_done: bool,
}
