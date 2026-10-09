// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub fn redact_tree(tree: &str, dialect: Dialect) -> Redacted {
    let mut out = Redacted {
        tree: String::with_capacity(tree.len()),
        nodes: Vec::new(),
    };
    let mut node = String::new();
    for line in tree.split_inclusive('\n') {
        if !node.is_empty() && dialect.starts_node(line) {
            push_node(&mut out, &node, dialect);
            node.clear();
        }
        node.push_str(line);
    }
    push_node(&mut out, &node, dialect);
    out
}
