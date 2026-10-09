// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A tree with every secret's value taken out, and what is known of each
/// element in it that can be acted on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redacted {
    pub tree: String,
    /// In tree order.
    pub nodes: Vec<TreeNode>,
}

/// One element that can be acted on: a node whose line carries `[N]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub index: u32,
    /// Where its line starts in [`Redacted::tree`], in bytes.
    pub offset: u32,
    /// Its role as the tree spells it: `AXTextField`, `Edit`, `password text`.
    pub role: String,
    /// A password or other secret field.
    pub secret: bool,
}

/// Take the value out of every tree node that describes a secret, and name
/// the elements that can be acted on.
///
/// The platforms already refuse to hand a secure field's text to an
/// accessibility client — macOS answers bullets for a secure text field,
/// Windows refuses a password edit's value to other processes — so this is
/// the second line, not the first: a node whose role names a password
/// control, or whose label says it is one, or whose value is nothing but
/// bullets, keeps its role and label and loses its value. The driver's tree
/// does not carry macOS subroles, so a secure field is recognised here by its
/// words; recognising it by subrole needs the driver to report one.
pub fn redact_secrets(tree: &str) -> Redacted {
    redact_tree(tree, Dialect::current())
}

/// The roles of an application's menu bars in the macOS tree — its menus
/// and its status items — and of the items on them.
pub const APP_MENU_ROLES: &[&str] = &["AXMenuBar", "AXExtrasMenuBar", "AXMenuBarItem"];

/// How much of its application's menu bars a window's tree keeps (macOS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMenus<'a> {
    /// None of them: a window shared on its own does not reach its
    /// application's menus.
    Withheld,
    /// The application's own menus, for an application shared as a whole —
    /// but not the Apple menu and the application menu, the first two items
    /// of its menu bar, named here by their titles as Accessibility gives
    /// them (a tree cut down by a query may not show either, and a row's
    /// place in it says nothing): they restart and log out, run Services and
    /// hide every other application, reaching past this one.
    Own { protected: &'a [String; 2] },
}

/// The tree without what `keep` leaves out of its application's menu bars,
/// and the indices of the elements that were in it (macOS).
///
/// The driver's tree of a window carries its application's menu bars along
/// with it. A menu bar's row goes with every line under it, up to the next
/// row no deeper than itself — and so does each item on it, row by row,
/// since the driver leaves out a menu bar's own row when it has nothing to
/// say, keeping its items at their depth. Elsewhere a window's menus are its
/// own, and the tree is left whole.
pub fn without_app_menus(
    tree: &str,
    dialect: Dialect,
    keep: AppMenus<'_>,
) -> (String, BTreeSet<u32>) {
    let mut withheld = BTreeSet::new();
    if dialect != Dialect::Mac {
        return (tree.to_string(), withheld);
    }
    let mut out = String::with_capacity(tree.len());
    // The depth of what is being left out, while something is.
    let mut leaving: Option<usize> = None;
    for line in tree.split_inclusive('\n') {
        if let Some(head) = dialect.head(line) {
            let depth = line.len() - line.trim_start_matches(' ').len();
            if leaving.is_some_and(|start| depth <= start) {
                leaving = None;
            }
            if leaving.is_none() {
                let leave = match (head.role, keep) {
                    ("AXMenuBar" | "AXExtrasMenuBar" | "AXMenuBarItem", AppMenus::Withheld) => true,
                    ("AXMenuBarItem", AppMenus::Own { protected }) => protected
                        .iter()
                        .any(|title| line.contains(&format!("AXMenuBarItem \"{title}\""))),
                    _ => false,
                };
                if leave {
                    leaving = Some(depth);
                }
            }
            if leaving.is_some() {
                withheld.extend(head.index);
            }
        }
        if leaving.is_none() {
            out.push_str(line);
        }
    }
    (out, withheld)
}

/// The shape of a node line in the driver's tree on each platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// `- [3] AXTextField "Title" = "value" (description) [attrs]`
    Mac,
    /// `- [3] Edit "Name" [value="…" id=… actions=[…]]`, and `- Text "Name" = "…"`
    Windows,
    /// `- [3] password text "name" value="…" [actions=[…]]`, and `- label = "name"`
    Linux,
}

/// What a node's first line says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeHead<'a> {
    /// The `N` of `[N]`: only elements that can be acted on carry one.
    pub index: Option<u32>,
    pub role: &'a str,
}
