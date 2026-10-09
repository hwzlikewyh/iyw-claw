// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Where the text after a node's value begins, as near as can be told: the
/// driver does not escape the quote that closes a value, so this is the
/// earliest quote on the node's last line (a value ends on the line its node
/// does) that is followed by what only comes after one in this platform's
/// tree, or the node's closing quote. Earlier is the safe side — more of the
/// node is read as label.
pub(super) fn after_value(node: &str, value_from: usize, dialect: Dialect) -> usize {
    let body = node.trim_end_matches('\n');
    let last_line = body.rfind('\n').map_or(0, |i| i + 1);
    let from = last_line.max(value_from).min(body.len());
    let tail = &body[from..];
    dialect
        .after_value()
        .iter()
        .filter_map(|m| tail.find(m))
        .chain(tail.ends_with('"').then(|| tail.len() - 1))
        .min()
        .map_or(body.len(), |i| from + i)
}
