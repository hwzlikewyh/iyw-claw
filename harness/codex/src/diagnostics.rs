use std::sync::OnceLock;
use std::time::Instant;

use regex::Regex;

const MAX_DETAIL_CHARS: usize = 900;
const MAX_SCAN_CHARS: usize = 16 * 1024;

pub(crate) fn safe_detail(value: &str) -> String {
    let mut value = value.chars().take(MAX_SCAN_CHARS).collect::<String>();
    for (pattern, replacement) in redaction_rules() {
        value = pattern.replace_all(&value, *replacement).into_owned();
    }
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_DETAIL_CHARS)
        .collect()
}

fn redaction_rules() -> &'static Vec<(Regex, &'static str)> {
    static RULES: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    RULES.get_or_init(|| {
        [
            (r"\x1b\[[0-?]*[ -/]*[@-~]", ""),
            (r"(?m)^\s*\d+\s*\|.*$", "[source redacted]"),
            (r#"(?i)\b(?:authorization|cookie|set-cookie)\b["'\s]*[:=].*"#, "[credential redacted]"),
            (r"(?i)\bbearer\s+\S+", "Bearer [redacted]"),
            (r#"(?i)\b[\w-]*(?:key|token|secret|password|passwd|credential|signature)\b["'\s]*[:=]\s*(?:"[^"\r\n]*"|'[^'\r\n]*'|\S+)"#, "[credential redacted]"),
            (r#"(?i)\b[\w-]*(?:key|token|secret|password|credential)\b\s+(?:provided|is)?\s*[:=]?\s*['"][^'"\r\n]+['"]"#, "[credential redacted]"),
            (r#"[A-Za-z][A-Za-z0-9+.-]*://[^\s"'<>]+"#, "[url redacted]"),
            (r"\b(?:sk-|gh[pousr]_|xox[abprs]-)[A-Za-z0-9_-]{8,}", "[credential redacted]"),
            (r"\beyJ[A-Za-z0-9_-]{10,}(?:\.[A-Za-z0-9_-]{10,})*", "[credential redacted]"),
            (r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*", "[private key redacted]"),
            (r"\b[A-Za-z0-9_.+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b", "[email redacted]"),
            (r#"(?m)(^|[\s=(\[{"'])(~[/\\][^\r\n"'<>]+|[A-Za-z]:[/\\][^\r\n"'<>|]+|\\\\[^\r\n"'<>|]+|/[^/\s"'<>]+(?:/[^\s"'<>]+)+)"#, "${1}[path redacted]"),
        ].into_iter().map(|(pattern, replacement)| {
            (Regex::new(pattern).expect("static diagnostic regex"), replacement)
        }).collect()
    })
}

pub(crate) struct StartupStage {
    name: &'static str,
    started: Instant,
    finished: bool,
}

impl StartupStage {
    pub(crate) fn new(name: &'static str) -> Self {
        eprintln!("[internal-codex-worker] stage={name} status=begin");
        Self {
            name,
            started: Instant::now(),
            finished: false,
        }
    }

    pub(crate) fn complete(mut self) {
        self.finished = true;
        eprintln!(
            "[internal-codex-worker] stage={} status=ok elapsed_ms={}",
            self.name,
            self.started.elapsed().as_millis()
        );
    }

    pub(crate) fn finish<T, E: std::fmt::Display>(self, result: Result<T, E>) -> Result<T, String> {
        match result {
            Ok(value) => {
                self.complete();
                Ok(value)
            }
            Err(error) => Err(self.fail(error)),
        }
    }

    pub(crate) fn fail(mut self, error: impl std::fmt::Display) -> String {
        self.finished = true;
        let detail = safe_detail(&error.to_string());
        eprintln!(
            "[internal-codex-worker] stage={} status=error elapsed_ms={} detail={detail}",
            self.name,
            self.started.elapsed().as_millis()
        );
        format!("{}: {detail}", self.name)
    }
}

impl Drop for StartupStage {
    fn drop(&mut self) {
        if !self.finished {
            eprintln!(
                "[internal-codex-worker] stage={} status=interrupted elapsed_ms={}",
                self.name,
                self.started.elapsed().as_millis()
            );
        }
    }
}
