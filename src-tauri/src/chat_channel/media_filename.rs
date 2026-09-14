use std::sync::OnceLock;

use regex::Regex;

pub fn from_disposition(header: &str) -> Option<String> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r#"(?i)(?:^|;)\s*(filename\*?)\s*=\s*(?:"((?:\\.|[^"\\])*)"|([^;]*))"#)
            .expect("valid content-disposition filename pattern")
    });
    let mut plain = None;
    for capture in pattern.captures_iter(header) {
        let value = capture.get(2).or_else(|| capture.get(3))?.as_str().trim();
        if capture[1].ends_with('*') {
            let mut parts = value.splitn(3, '\'');
            if !parts.next()?.eq_ignore_ascii_case("utf-8") {
                continue;
            }
            parts.next()?;
            let decoded = urlencoding::decode(parts.next()?).ok()?;
            return Some(crate::commands::chat_attachments::sanitize_file_name(
                &decoded,
            ));
        }
        plain = Some(crate::commands::chat_attachments::sanitize_file_name(value));
    }
    plain
}
