use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use super::{parse_execution, OpencliFailure, OpencliFailureKind};
use crate::commands::internet_tools::OpencliExecution;

const MAX_DETAIL_CHARS: usize = 768;

pub(super) fn check_execution(execution: &OpencliExecution) -> Result<Value, OpencliFailure> {
    let normalized = OpencliExecution {
        success: execution.success,
        exit_code: execution.exit_code,
        stdout: plain_text(&execution.stdout),
        stderr: plain_text(&execution.stderr),
    };
    let value = parse_execution(&normalized).map_err(|mut failure| {
        failure.message = safe_detail(&failure.message);
        log_failure(&failure);
        failure
    })?;
    let report = DoctorReport::parse(&value);
    if report.healthy() {
        return Ok(value);
    }
    let (reason, hint, kind) = report.failure();
    let details = safe_detail(&report.detail);
    let failure = OpencliFailure {
        code: match kind {
            OpencliFailureKind::Runtime => "OPENCLI_RUNTIME_FAILED",
            _ => "OPENCLI_BRIDGE_UNAVAILABLE",
        }
        .into(),
        message: format!("OpenCLI doctor: {reason}. {hint} Diagnostic: {details}"),
        kind,
    };
    log_failure(&failure);
    Err(failure)
}

#[derive(Default)]
struct DoctorReport {
    daemon: Option<bool>,
    extension: Option<bool>,
    connectivity: Option<bool>,
    detail: String,
    profile_required: bool,
    profile_disconnected: bool,
}

impl DoctorReport {
    fn parse(value: &Value) -> Self {
        let mut report = match value {
            Value::Object(_) => Self::from_json(value),
            Value::String(text) => Self::from_text(text),
            _ => Self::default(),
        };
        let text = report.detail.to_ascii_lowercase();
        report.profile_required = text.contains("multiple chrome profiles")
            || text.contains("profile_required")
            || text.contains("profile-required");
        report.profile_disconnected = text.contains("selected browser profile is not connected")
            || text.contains("profile_disconnected")
            || text.contains("profile-disconnected");
        report
    }

    fn from_json(value: &Value) -> Self {
        let mut details = value
            .get("issues")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        if let Some(error) = value.pointer("/connectivity/error").and_then(Value::as_str) {
            details.push(error);
        }
        Self {
            daemon: value.get("daemonRunning").and_then(Value::as_bool),
            extension: value.get("extensionConnected").and_then(Value::as_bool),
            connectivity: value.pointer("/connectivity/ok").and_then(Value::as_bool),
            detail: details.join("; "),
            ..Self::default()
        }
    }

    fn from_text(text: &str) -> Self {
        let text = plain_text(text);
        let mut report = Self::default();
        let mut details = Vec::new();
        for line in text.lines().map(str::trim) {
            if let Some((label, healthy)) = status_line(line) {
                match label.split_once(':').map(|(label, _)| label) {
                    Some("Daemon") => report.daemon = Some(healthy),
                    Some("Extension") => report.extension = Some(healthy),
                    Some("Connectivity") => report.connectivity = Some(healthy),
                    _ => continue,
                }
                details.push(line);
            } else if line.contains("Multiple Chrome profiles")
                || line.contains("Selected browser profile is not connected")
            {
                details.push(line);
            }
        }
        report.detail = if details.is_empty() {
            text.lines().take(3).collect::<Vec<_>>().join("; ")
        } else {
            details.join("; ")
        };
        report
    }

    fn healthy(&self) -> bool {
        self.daemon == Some(true) && self.extension == Some(true) && self.connectivity == Some(true)
    }

    fn failure(&self) -> (&'static str, &'static str, OpencliFailureKind) {
        use OpencliFailureKind::{BridgeUnavailable, Runtime};
        if self.daemon == Some(false) {
            return (
                "daemon_not_running",
                "Run opencli doctor to start and check the daemon.",
                BridgeUnavailable,
            );
        }
        if self.profile_required {
            return ("profile_selection_required", "Use opencli profile list and select the intended profile with opencli profile use <name>.", BridgeUnavailable);
        }
        if self.profile_disconnected {
            return ("selected_profile_disconnected", "Open the selected Chrome profile, or select a connected profile with opencli profile use <name>.", BridgeUnavailable);
        }
        if self.extension == Some(false) {
            return (
                "extension_disconnected",
                "Open the intended Chrome profile and check its OpenCLI extension connection.",
                BridgeUnavailable,
            );
        }
        if self.connectivity == Some(false) {
            return (
                "connectivity_failed",
                "The live browser probe failed; inspect the diagnostic before retrying.",
                BridgeUnavailable,
            );
        }
        ("unrecognized_doctor_report", "Check the managed OpenCLI version and doctor output format; this does not prove the extension is disconnected.", Runtime)
    }
}

fn status_line(line: &str) -> Option<(&str, bool)> {
    for (prefix, healthy) in [("[OK]", true), ("[FAIL]", false), ("[MISSING]", false)] {
        if let Some(label) = line.strip_prefix(prefix) {
            return Some((label.trim_start(), healthy));
        }
    }
    let label = line.strip_prefix("[WARN]")?.trim_start();
    // 版本提示不阻断连接，doctor 已明确记录的不稳定状态仍视为不可用。
    let healthy = label.starts_with("Daemon: running") || label.starts_with("Extension: connected");
    Some((label, healthy))
}

fn plain_text(text: &str) -> String {
    static ANSI: OnceLock<Regex> = OnceLock::new();
    ANSI.get_or_init(|| Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]").expect("ANSI CSI pattern"))
        .replace_all(text, "")
        .into_owned()
}

fn safe_detail(text: &str) -> String {
    let sanitized = crate::acp::stderr_tail::sanitize_diagnostic(&plain_text(text));
    let text = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return "no diagnostic detail".into();
    }
    text.chars()
        .filter(|ch| !ch.is_control())
        .take(MAX_DETAIL_CHARS)
        .collect()
}

fn log_failure(failure: &OpencliFailure) {
    tracing::warn!(target: "iyw_claw_browser", code = %failure.code,
        reason = %failure.message, "OpenCLI browser preflight failed");
}
