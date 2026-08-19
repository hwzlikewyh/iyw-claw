//! Per-turn output diagnosis for silent ACP completions.

use std::fmt::Display;
use std::time::{Duration, Instant};

use crate::acp::stderr_tail::{summarize_parser_error, StderrTail};
use crate::models::agent::AgentType;

const DROP_LOG_WINDOW: Duration = Duration::from_secs(10);
const EMPTY_TURN_STDERR_LINES: usize = 12;
const EMPTY_TURN_STDERR_BYTES: usize = 900;
const MAX_DETAILS_BYTES: usize = 1_200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropSite {
    Decode,
    Dispatch,
}

impl DropSite {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Decode => "decode",
            Self::Dispatch => "dispatch",
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct DropLogThrottle {
    last_emit: Option<Instant>,
    occurrences: u64,
}

impl DropLogThrottle {
    pub(crate) fn record(&mut self, site: &str, error: &impl Display) {
        self.occurrences = self.occurrences.saturating_add(1);
        let now = Instant::now();
        if self
            .last_emit
            .is_some_and(|last| now.saturating_duration_since(last) < DROP_LOG_WINDOW)
        {
            return;
        }
        let occurrences = std::mem::take(&mut self.occurrences);
        self.last_emit = Some(now);
        let summary = summarize_parser_error(&error.to_string());
        tracing::warn!(
            site,
            occurrences,
            window_seconds = DROP_LOG_WINDOW.as_secs(),
            error = %summary,
            "[ACP] dropped unreadable session update"
        );
    }
}

#[derive(Debug, Default)]
pub(crate) struct TurnOutputProbe {
    saw_agent_output: bool,
    saw_metadata_update: bool,
    dropped_decode: u32,
    dropped_dispatch: u32,
    first_drop: Option<(DropSite, String)>,
    stderr_mark: u64,
}

impl TurnOutputProbe {
    pub(crate) fn new(stderr_mark: u64) -> Self {
        Self {
            stderr_mark,
            ..Self::default()
        }
    }

    pub(crate) fn note_update(&mut self, is_agent_output: bool) {
        if is_agent_output {
            self.saw_agent_output = true;
        } else {
            self.saw_metadata_update = true;
        }
    }

    pub(crate) fn note_dropped(&mut self, site: DropSite, error: &impl Display) {
        match site {
            DropSite::Decode => self.dropped_decode = self.dropped_decode.saturating_add(1),
            DropSite::Dispatch => self.dropped_dispatch = self.dropped_dispatch.saturating_add(1),
        }
        if self.first_drop.is_none() {
            self.first_drop = Some((site, summarize_parser_error(&error.to_string())));
        }
    }

    pub(crate) const fn saw_agent_output(&self) -> bool {
        self.saw_agent_output
    }

    fn dropped_total(&self) -> u32 {
        self.dropped_decode.saturating_add(self.dropped_dispatch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmptyTurnCause {
    ProtocolMismatch,
    MetadataOnly,
    NoOutput,
}

impl EmptyTurnCause {
    const fn code(self) -> &'static str {
        match self {
            Self::NoOutput => "turn_failed_empty",
            Self::ProtocolMismatch => "turn_failed_empty_protocol",
            Self::MetadataOnly => "turn_failed_empty_metadata",
        }
    }

    fn message(self, agent_type: AgentType) -> String {
        match self {
            Self::NoOutput => format!("{agent_type} ended the turn without producing a response."),
            Self::ProtocolMismatch => format!(
                "{agent_type} produced output that iyw-claw could not parse; the agent version may not match the protocol."
            ),
            Self::MetadataOnly => {
                format!("{agent_type} sent only status updates this turn and no reply.")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EmptyTurnReport {
    cause: EmptyTurnCause,
    details: Option<String>,
}

impl EmptyTurnReport {
    pub(crate) const fn code(&self) -> &'static str {
        self.cause.code()
    }

    pub(crate) fn message(&self, agent_type: AgentType) -> String {
        self.cause.message(agent_type)
    }

    pub(crate) fn details(&self) -> Option<String> {
        self.details.clone()
    }
}

fn diagnose_empty_turn(probe: &TurnOutputProbe) -> EmptyTurnCause {
    if probe.dropped_total() > 0 {
        EmptyTurnCause::ProtocolMismatch
    } else if probe.saw_metadata_update {
        EmptyTurnCause::MetadataOnly
    } else {
        EmptyTurnCause::NoOutput
    }
}

fn build_empty_turn_details(probe: &TurnOutputProbe, stderr_tail: &StderrTail) -> Option<String> {
    let mut sections = Vec::new();
    if probe.dropped_total() > 0 {
        sections.push(drop_details(probe));
    }
    if let Some(stderr) = stderr_details(probe, stderr_tail) {
        sections.push(stderr);
    }
    if sections.is_empty() {
        None
    } else {
        Some(truncate_details(&sections.join("\n")))
    }
}

fn drop_details(probe: &TurnOutputProbe) -> String {
    let mut detail = format!(
        "dropped {} update(s) ({} decode, {} dispatch)",
        probe.dropped_total(),
        probe.dropped_decode,
        probe.dropped_dispatch
    );
    if let Some((site, summary)) = &probe.first_drop {
        detail.push_str(&format!("; first ({}): {summary}", site.label()));
    }
    detail
}

fn stderr_details(probe: &TurnOutputProbe, stderr_tail: &StderrTail) -> Option<String> {
    let tail = stderr_tail.tail_since(
        probe.stderr_mark,
        EMPTY_TURN_STDERR_LINES,
        EMPTY_TURN_STDERR_BYTES,
    );
    if tail.is_empty() {
        return None;
    }
    let mut block = format!("stderr (this turn, last {} lines):", tail.lines.len());
    for line in tail.lines {
        block.push_str("\n  ");
        block.push_str(&line);
    }
    Some(block)
}

fn truncate_details(value: &str) -> String {
    if value.len() <= MAX_DETAILS_BYTES {
        return value.to_string();
    }
    const MARKER: &str = "...";
    let mut end = MAX_DETAILS_BYTES - MARKER.len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{MARKER}", &value[..end])
}

pub(crate) fn finish_turn_reason<'a>(
    probe: &TurnOutputProbe,
    raw_reason: &'a str,
    stderr_tail: &StderrTail,
    diagnose_empty: bool,
) -> (&'a str, Option<EmptyTurnReport>) {
    if !diagnose_empty || raw_reason != "end_turn" || probe.saw_agent_output() {
        return (raw_reason, None);
    }
    let report = EmptyTurnReport {
        cause: diagnose_empty_turn(probe),
        details: build_empty_turn_details(probe, stderr_tail),
    };
    ("empty", Some(report))
}
