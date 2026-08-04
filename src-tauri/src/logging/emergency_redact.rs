use std::fmt::Display;

const MAX_DETAIL_BYTES: usize = 4096;

pub(super) fn redact(value: impl Display) -> String {
    let mut text = value.to_string();
    for marker in [
        "token=",
        "access_token=",
        "password=",
        "secret=",
        "api_key=",
        "authorization=",
        "authorization:",
    ] {
        text = redact_marker(&text, marker);
    }
    if text.contains("http") {
        if let Some(query_start) = text.find('?') {
            text.truncate(query_start);
            text.push_str("?<redacted>");
        }
    }
    let mut clean = text
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .collect::<String>();
    if clean.len() > MAX_DETAIL_BYTES {
        let mut boundary = MAX_DETAIL_BYTES;
        while boundary > 0 && !clean.is_char_boundary(boundary) {
            boundary -= 1;
        }
        clean.truncate(boundary);
        clean.push_str("...<truncated>");
    }
    clean
}

fn redact_marker(value: &str, marker: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let Some(start) = lower.find(marker) else {
        return value.to_string();
    };
    let end = value[start + marker.len()..]
        .find(char::is_whitespace)
        .map(|offset| start + marker.len() + offset)
        .unwrap_or(value.len());
    format!(
        "{}<redacted>{}",
        &value[..start + marker.len()],
        &value[end..]
    )
}
