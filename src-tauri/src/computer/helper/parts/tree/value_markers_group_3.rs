// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Dialect {
    /// Where a node's value starts in this platform's tree: after ` = ` on
    /// every macOS node and on the other platforms' plain nodes; in
    /// `[value="…"` (Windows) or ` value="…"` (Linux) on their addressable
    /// ones. Only this platform's: another's marker in a title is just text.
    pub(in crate::computer::helper::tree) fn value_markers(self) -> &'static [&'static str] {
        match self {
            Dialect::Mac => &[" = \""],
            Dialect::Windows => &[" = \"", " [value=\""],
            Dialect::Linux => &[" = \"", " value=\""],
        }
    }

    /// What can only follow the quote that closes a value in this platform's
    /// tree — a description or an attribute block (macOS), the next attribute
    /// in the block the value sits in (Windows), the attribute block (Linux) —
    /// besides the end of the node.
    pub(in crate::computer::helper::tree) fn after_value(self) -> &'static [&'static str] {
        match self {
            Dialect::Mac => &["\" (", "\" ["],
            Dialect::Windows => &["\" id=", "\" help=", "\" actions=", "\"]"],
            Dialect::Linux => &["\" [actions="],
        }
    }

    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Dialect::Mac
        } else if cfg!(windows) {
            Dialect::Windows
        } else {
            Dialect::Linux
        }
    }

    /// Whether `line` starts a node rather than continuing the value of the
    /// one above it.
    pub fn starts_node(self, line: &str) -> bool {
        self.head(line).is_some()
    }

    /// The head of the node `line` starts, if it starts one: indentation in
    /// whole steps, `- `, an optional `[index] `, then a role as this
    /// platform's tree spells one — an `AX` role on macOS, one of UI
    /// Automation's control types on Windows, an AT-SPI role name followed by
    /// the quoted name every Linux node carries. A value's own lines are the
    /// user's text; the stricter this is, the less of that text can pass for
    /// a node and escape its node's redaction.
    pub fn head(self, line: &str) -> Option<NodeHead<'_>> {
        let line = line.trim_end_matches('\n');
        let rest = line.trim_start_matches(' ');
        if !(line.len() - rest.len()).is_multiple_of(2) {
            return None;
        }
        let mut rest = rest.strip_prefix("- ")?;
        let mut index = None;
        if let Some(inner) = rest.strip_prefix('[') {
            let close = inner.find("] ")?;
            if close == 0 || !inner[..close].bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            index = Some(inner[..close].parse::<u32>().ok()?);
            rest = &inner[close + 2..];
        }
        // What may follow a role in a node line: nothing, a quoted title or
        // name, a value, a description, an attribute block.
        let follows = |after: &str, allowed: &[&str]| {
            after.is_empty() || allowed.iter().any(|a| after.starts_with(a))
        };
        let role = match self {
            Dialect::Mac => {
                let end = rest
                    .bytes()
                    .position(|b| !b.is_ascii_alphanumeric())
                    .unwrap_or(rest.len());
                (rest.starts_with("AX")
                    && end > 2
                    && follows(&rest[end..], &[" \"", " = \"", " (", " ["]))
                .then(|| &rest[..end])
            }
            Dialect::Windows => {
                let end = rest
                    .bytes()
                    .position(|b| !b.is_ascii_alphabetic())
                    .unwrap_or(rest.len());
                (UIA_CONTROL_TYPES.contains(&&rest[..end])
                    && follows(&rest[end..], &[" \"", " = \"", " ["]))
                .then(|| &rest[..end])
            }
            Dialect::Linux => {
                // `- [3] push button "name" …` and `- label = "name"`: an
                // AT-SPI node always carries its name, quoted.
                let end = rest
                    .bytes()
                    .position(|b| !(b.is_ascii_lowercase() || b == b' '))
                    .unwrap_or(rest.len());
                let role = rest[..end].trim_end();
                let after = &rest[role.len()..];
                (!role.is_empty()
                    && after.starts_with(if index.is_some() { " \"" } else { " = \"" }))
                .then_some(role)
            }
        }?;
        Some(NodeHead { index, role })
    }
}
