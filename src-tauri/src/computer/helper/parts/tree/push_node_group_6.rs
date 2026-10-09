// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) fn push_node(out: &mut Redacted, node: &str, dialect: Dialect) {
    let offset = u32::try_from(out.tree.len()).unwrap_or(u32::MAX);
    let (text, secret) = redact_node(node, dialect);
    let first_line = node.split_inclusive('\n').next().unwrap_or("");
    if let Some(NodeHead {
        index: Some(index),
        role,
    }) = dialect.head(first_line)
    {
        out.nodes.push(TreeNode {
            index,
            offset,
            role: role.to_string(),
            secret,
        });
    }
    out.tree.push_str(&text);
}

/// One node, redacted if it is a secret, and whether it is.
pub(super) fn redact_node(node: &str, dialect: Dialect) -> (String, bool) {
    // The earliest marker: what is kept of a redacted node ends there, so it
    // can never hold any of the value.
    let marker = dialect
        .value_markers()
        .iter()
        .filter_map(|m| node.find(m).map(|i| (i, *m)))
        .min_by_key(|(i, _)| *i);
    // Judged on what names the node — its role and title before the value,
    // its description and attributes after it — never on the value itself: a
    // document that mentions a password is not a password field, and a
    // field's secret is not what says it is one. The one exception is a value
    // that is nothing but bullets, which is how a secure field shows its
    // contents.
    let (label, value) = match marker {
        Some((start, m)) => {
            let from = start + m.len();
            let after = after_value(node, from, dialect).max(from);
            (
                format!("{}{}", &node[..start], &node[after..]),
                Some(&node[from..after]),
            )
        }
        None => (node.to_string(), None),
    };
    let masked = value.is_some_and(is_masked);
    let secret = names_a_secret(&label) || masked;
    let Some((start, _)) = marker.filter(|_| secret) else {
        return (node.to_string(), secret);
    };
    // Everything from the value on goes, not just the value: the driver writes
    // values unescaped, so a value can contain `" (` or `" [` or a newline and
    // there is no telling where it ends. What stays — the index, the role and
    // the title — is what names the field.
    let newline = if node.ends_with('\n') { "\n" } else { "" };
    (
        format!(
            "{} = \"[redacted]\"{newline}",
            node[..start].trim_end_matches([' ', '['])
        ),
        true,
    )
}
