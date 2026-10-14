use super::validation::{source_hash, valid_name};
use serde_json::Value;

pub struct Fence {
    pub body: String,
    pub key: String,
    pub hash: String,
}

pub fn scan(text: &str) -> Vec<Fence> {
    let mut output = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let Some((marker, count, language, indent)) = opening(line) else {
            continue;
        };
        let mut body = Vec::new();
        let mut closed = false;
        for line in lines.by_ref() {
            if closing(line, marker, count) {
                closed = true;
                break;
            }
            let leading = line.len() - line.trim_start_matches(' ').len();
            body.push(&line[indent.min(leading)..]);
        }
        if !closed || !matches!(language.as_str(), "iyw-ui" | "iyw-widget") {
            continue;
        }
        let source = body.join("\n").trim().to_owned();
        let Some(key) = block_key(&source, &language) else {
            continue;
        };
        output.push(Fence {
            hash: source_hash(&source),
            body: source,
            key,
        });
    }
    output
}

fn opening(line: &str) -> Option<(char, usize, String, usize)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let count = trimmed.chars().take_while(|c| *c == marker).count();
    if count < 3 {
        return None;
    }
    let info = &trimmed[count..];
    if marker == '`' && info.contains('`') {
        return None;
    }
    Some((
        marker,
        count,
        info.split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase(),
        line.len() - trimmed.len(),
    ))
}

fn closing(line: &str, marker: char, minimum: usize) -> bool {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return false;
    }
    let count = trimmed.chars().take_while(|c| *c == marker).count();
    count >= minimum && trimmed[count..].trim().is_empty()
}

fn block_key(source: &str, language: &str) -> Option<String> {
    if language == "iyw-widget" {
        return Some(format!("widget-{}", hash_text(source)));
    }
    let value: Value = serde_json::from_str(source).ok()?;
    value.get("component")?.as_str()?;
    let id = match value.get("id") {
        Some(Value::String(id)) => id.trim().to_owned(),
        Some(Value::Number(id)) => id.to_string(),
        _ => String::new(),
    };
    Some(if valid_name(&id) {
        id
    } else {
        format!("auto-{}", hash_text(source))
    })
}

fn hash_text(text: &str) -> String {
    let hash = text.encode_utf16().fold(0x811c9dc5_u32, |hash, unit| {
        (hash ^ u32::from(unit)).wrapping_mul(0x01000193)
    });
    let mut value = hash;
    let mut chars = Vec::new();
    loop {
        chars.push(char::from_digit(value % 36, 36).unwrap_or('0'));
        value /= 36;
        if value == 0 {
            break;
        }
    }
    chars.into_iter().rev().collect()
}
