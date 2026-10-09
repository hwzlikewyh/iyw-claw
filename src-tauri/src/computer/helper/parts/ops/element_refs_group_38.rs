// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The elements of a snapshot that can be acted on, and what is known of each.
///
/// The driver's structured `elements` are the authority on which elements
/// exist and what their roles are; the tree lines only say where each is and
/// how the redaction judged it. The two are joined by index, and the tree is
/// not trusted on its own: the driver writes values unescaped, so a line of
/// some field's text can look exactly like `- [7] AXButton "OK"`. So:
///
/// * a ref is offered only for an element the driver listed whose index
///   heads exactly one tree line — an index that heads two (one of them
///   forged by a value) cannot be told apart, and is offered for neither;
/// * an element is secret if ANY line carrying its index was judged secret,
///   or its own role, label or value say so — a forged line can add secrecy,
///   never take it away.
pub(super) fn element_refs(
    nodes: &[TreeNode],
    elements: Option<&Value>,
) -> (Vec<SnapshotRef>, HashMap<u32, ElementFacts>) {
    let mut lines: HashMap<u32, Vec<&TreeNode>> = HashMap::new();
    for node in nodes {
        lines.entry(node.index).or_default().push(node);
    }
    let mut refs = Vec::new();
    let mut facts = HashMap::new();
    for element in elements.and_then(Value::as_array).into_iter().flatten() {
        let Some(index) = element
            .get("element_index")
            .and_then(Value::as_u64)
            .and_then(|i| u32::try_from(i).ok())
        else {
            continue;
        };
        let text = |key: &str| element.get(key).and_then(Value::as_str).unwrap_or("");
        let role = text("role");
        let own_lines = lines.get(&index).map(Vec::as_slice).unwrap_or(&[]);
        let secret = own_lines.iter().any(|n| n.secret)
            || names_a_secret(&format!("{role} {}", text("label")))
            || is_masked(text("value"))
            // An index the tree cannot place is not trusted to be harmless.
            || own_lines.len() > 1;
        if let [line] = own_lines {
            refs.push(SnapshotRef {
                index,
                offset: line.offset,
                secret,
            });
        }
        facts.insert(
            index,
            ElementFacts {
                role: role.to_string(),
                secret,
                paste: names_a_paste_control(role, text("label")),
                frame: element_frame(element),
            },
        );
    }
    refs.sort_by_key(|r| r.offset);
    (refs, facts)
}

/// Whether an element is a menu command or a button named for pasting: one
/// pressed writes the person's clipboard into the window, as ⌘V / Ctrl+V
/// does. Roles as the three platforms' trees spell them.
pub(super) fn names_a_paste_control(role: &str, label: &str) -> bool {
    // Exactly these: a tab is a radio button, and "Pastebin" one to select.
    const PRESSED: &[&str] = &[
        "axmenuitem",
        "axbutton",
        "menuitem",
        "button",
        "splitbutton",
        "menu item",
        "push button",
    ];
    PRESSED.contains(&role.to_ascii_lowercase().as_str())
        && crate::computer::keys::names_paste(label)
}

/// The driver's `frame` of one element (`{x, y, w, h}`, in desktop units),
/// when it gave a whole one with an area.
pub(super) fn element_frame(element: &Value) -> Option<Rect> {
    let frame = element.get("frame")?;
    let number = |key: &str| {
        frame
            .get(key)
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
    };
    let rect = Rect {
        x: number("x")?,
        y: number("y")?,
        width: number("w")?,
        height: number("h")?,
    };
    (!rect.is_empty()).then_some(rect)
}
