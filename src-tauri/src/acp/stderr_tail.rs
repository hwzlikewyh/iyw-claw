//! Redacted, bounded private-host Agent stderr evidence.
//!
//! A shared ACP runtime captures stderr only during startup. Once initialized,
//! capture is disabled because process stderr cannot safely be attributed to a
//! particular session.

use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};

use regex::Regex;

const MAX_LINES: usize = 200;
const MAX_BYTES: usize = 32 * 1024;
const MAX_LINE_BYTES: usize = 512;
const MAX_SCAN_BYTES: usize = 16 * 1024;
const MAX_PARSE_SUMMARY_BYTES: usize = 200;

#[derive(Debug, Clone)]
pub struct TailSlice {
    pub lines: Vec<String>,
}

impl TailSlice {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[derive(Debug)]
pub struct StderrTail {
    capture_enabled: AtomicBool,
    startup_only: bool,
    inner: Mutex<TailInner>,
}

#[derive(Debug, Default)]
struct TailInner {
    lines: VecDeque<(u64, String)>,
    next_seq: u64,
    bytes: usize,
}

impl StderrTail {
    pub fn new() -> Self {
        Self {
            capture_enabled: AtomicBool::new(true),
            startup_only: false,
            inner: Mutex::new(TailInner::default()),
        }
    }

    /// Shared runtime hosts serve multiple sessions. Do not retain process
    /// stderr there because it cannot be attributed to one session safely.
    pub fn disabled() -> Self {
        Self {
            capture_enabled: AtomicBool::new(false),
            startup_only: false,
            inner: Mutex::new(TailInner::default()),
        }
    }

    pub fn new_startup_only() -> Self {
        Self {
            capture_enabled: AtomicBool::new(true),
            startup_only: true,
            inner: Mutex::new(TailInner::default()),
        }
    }

    pub fn push(&self, raw: &str) {
        if !self.capture_enabled.load(Ordering::Acquire) {
            return;
        }
        let stripped = strip_ansi(raw);
        // This scan cap discards bytes that can never reach the 512-byte ring entry.
        // Redaction still runs before the retained line is Unicode-safely truncated.
        let scanned = truncate_utf8(stripped.trim_end(), MAX_SCAN_BYTES);
        let redacted = sanitize_diagnostic(&scanned);
        let line = truncate_utf8(&redacted, MAX_LINE_BYTES);
        if line.is_empty() {
            return;
        }

        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let seq = inner.next_seq;
        inner.next_seq = inner.next_seq.saturating_add(1);
        inner.bytes = inner.bytes.saturating_add(line.len());
        inner.lines.push_back((seq, line));
        trim_ring(&mut inner);
    }

    pub fn mark(&self) -> u64 {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .next_seq
    }

    pub fn tail_since(&self, mark: u64, max_lines: usize, max_bytes: usize) -> TailSlice {
        if !self.capture_enabled.load(Ordering::Acquire) {
            return TailSlice { lines: Vec::new() };
        }
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let current: Vec<&str> = inner
            .lines
            .iter()
            .filter(|(seq, _)| *seq >= mark)
            .map(|(_, line)| line.as_str())
            .collect();
        TailSlice {
            lines: bounded_tail(current, max_lines, max_bytes),
        }
    }

    pub fn disable(&self) {
        self.capture_enabled.store(false, Ordering::Release);
    }

    pub fn disable_after_start(&self) {
        if self.startup_only {
            self.disable();
        }
    }
}

fn trim_ring(inner: &mut TailInner) {
    while inner.lines.len() > MAX_LINES || inner.bytes > MAX_BYTES {
        let Some((_, removed)) = inner.lines.pop_front() else {
            break;
        };
        inner.bytes = inner.bytes.saturating_sub(removed.len());
    }
}

fn bounded_tail(source: Vec<&str>, max_lines: usize, max_bytes: usize) -> Vec<String> {
    let mut remaining = max_bytes;
    let mut lines = Vec::new();
    for line in source.into_iter().rev().take(max_lines) {
        let bounded = truncate_utf8(line, remaining);
        if bounded.is_empty() {
            break;
        }
        remaining = remaining.saturating_sub(bounded.len());
        lines.push(bounded);
    }
    lines.reverse();
    lines
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    const MARKER: &str = "...";
    if max_bytes <= MARKER.len() {
        return MARKER[..max_bytes].to_string();
    }
    let limit = max_bytes - MARKER.len();
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{MARKER}", &value[..end])
}

fn strip_ansi(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\u{1b}' {
            output.push(character);
            continue;
        }
        match chars.peek() {
            Some('[') => skip_csi(&mut chars),
            Some(']') => skip_osc(&mut chars),
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    output
}

fn skip_csi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    chars.next();
    for character in chars.by_ref() {
        if ('\u{40}'..='\u{7e}').contains(&character) {
            break;
        }
    }
}

fn skip_osc(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    chars.next();
    while let Some(character) = chars.next() {
        if character == '\u{7}' {
            break;
        }
        if character == '\u{1b}' && chars.peek() == Some(&'\\') {
            chars.next();
            break;
        }
    }
}

type Rules = Vec<(Regex, &'static str)>;

fn redaction_rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| {
        [
            (r"(?i)\bauthorization\s*[:=]\s*.*", "Authorization: ***"),
            (r"(?i)\b(set-)?cookie\s*[:=]\s*.*", "${1}cookie: ***"),
            (r"(?i)\bbearer\s+[A-Za-z0-9._\-]{8,}", "Bearer ***"),
            (
                r"(?i)\b([A-Za-z0-9_-]*(?:api[_-]?key|access[_-]?token|auth[_-]?token|refresh[_-]?token|client[_-]?secret|private[_-]?key|credential|signature|secret|passwd|password|token|key))\b\s*[:=]\s*\S+",
                "$1=***",
            ),
            (
                r"([A-Za-z][A-Za-z0-9+.\-]*://)[^/\s:@]+:[^/\s@]+@",
                "${1}***:***@",
            ),
            (
                r"([A-Za-z][A-Za-z0-9+.\-]*://)[^/\s:@]+:[^/\s@]{16,}",
                "${1}***:***",
            ),
            (r"sk-[A-Za-z0-9_\-]{12,}", "sk-***"),
            (r"\b(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}", "${1}_***"),
            (r"\bxox[abprs]-[A-Za-z0-9\-]{10,}", "xox*-***"),
            (r"\bAKIA[0-9A-Z]{16}\b", "AKIA***"),
            (r"\beyJ[A-Za-z0-9_\-]{10,}(?:\.[A-Za-z0-9_\-]{10,})*", "<jwt>"),
            (r"-----BEGIN [A-Z ]*PRIVATE KEY-----", "<private key>"),
            (r"(?m)^[A-Za-z0-9+/]{40,}={0,2}$", "<secret material>"),
            (
                r#"(?m)(^|[\s=(\[{\"'])(~[/\\][^\s\"'<>]+|[A-Za-z]:[/\\][^\s\"'<>|]+|\\\\[^\s\"'<>|]+|/[^/\s\"'<>]+(?:/[^\s\"'<>]+)+)"#,
                "${1}<path>",
            ),
        ]
        .into_iter()
        .filter_map(|(pattern, replacement)| Regex::new(pattern).ok().map(|re| (re, replacement)))
        .collect()
    })
}

pub fn sanitize_diagnostic(value: &str) -> String {
    redaction_rules()
        .iter()
        .fold(value.to_string(), |current, (regex, replacement)| {
            regex.replace_all(&current, *replacement).into_owned()
        })
}

pub fn summarize_parser_error(error: &str) -> String {
    let first = error.lines().next().unwrap_or("");
    let scanned = truncate_utf8(first, MAX_SCAN_BYTES);
    let normalized = scanned.split_whitespace().collect::<Vec<_>>().join(" ");
    let category = [
        "invalid type",
        "invalid value",
        "invalid length",
        "unknown variant",
        "unknown field",
        "missing field",
        "trailing characters",
        "expected value",
        "EOF while parsing",
    ]
    .into_iter()
    .find(|prefix| normalized.starts_with(prefix))
    .unwrap_or("unrecognized parse error");
    let position = parse_position(&normalized);
    let summary = match position {
        Some(position) => format!("{category} (details redacted) {position}"),
        None => format!("{category} (details redacted)"),
    };
    truncate_utf8(&summary, MAX_PARSE_SUMMARY_BYTES)
}

fn parse_position(value: &str) -> Option<&str> {
    static POSITION: OnceLock<Regex> = OnceLock::new();
    POSITION
        .get_or_init(|| Regex::new(r"at line \d+ column \d+").expect("static regex"))
        .find(value)
        .map(|matched| matched.as_str())
}
