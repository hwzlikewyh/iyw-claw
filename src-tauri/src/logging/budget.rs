//! Cross-restart byte budget for the daily rolling file sink.

use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const DEFAULT_MAX_BYTES_PER_DAY: u64 = 512 * 1024 * 1024;
pub(super) const MAX_BYTES_ENV: &str = "IYW_CLAW_LOG_MAX_BYTES";

const SECONDS_PER_DAY: i64 = 86_400;

fn unix_day_from_secs(seconds: i64) -> i32 {
    seconds.div_euclid(SECONDS_PER_DAY) as i32
}

fn current_unix_day() -> i32 {
    let now = SystemTime::now();
    let seconds = match now.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(error) => -(error.duration().as_secs() as i64),
    };
    unix_day_from_secs(seconds)
}

pub(super) fn resume_point(dir: &Path, prefix: &str, suffix: &str) -> (i32, u64) {
    let now = chrono::Utc::now();
    let day = unix_day_from_secs(now.timestamp());
    let file_name = format!("{prefix}.{}.{suffix}", now.format("%Y-%m-%d"));
    let existing_bytes = std::fs::metadata(dir.join(file_name))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    (day, existing_bytes)
}

pub(super) fn configured_max_bytes_per_day() -> Option<u64> {
    let Ok(raw) = std::env::var(MAX_BYTES_ENV) else {
        return Some(DEFAULT_MAX_BYTES_PER_DAY);
    };
    if raw.trim().is_empty() {
        return Some(DEFAULT_MAX_BYTES_PER_DAY);
    }
    match raw.trim().parse::<u64>() {
        Ok(0) => None,
        Ok(limit) => Some(limit),
        Err(_) => {
            eprintln!(
                "[logging] {MAX_BYTES_ENV}={raw:?} is invalid; using the default \
                 {DEFAULT_MAX_BYTES_PER_DAY} bytes/day"
            );
            Some(DEFAULT_MAX_BYTES_PER_DAY)
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Notice {
    Exhausted {
        limit: u64,
        written: u64,
    },
    Reopened {
        dropped_lines: u64,
        dropped_bytes: u64,
    },
}

impl Notice {
    fn message(self) -> String {
        match self {
            Notice::Exhausted { limit, written } => format!(
                "[logging] daily file budget reached ({written}/{limit} bytes); \
                 dropping file logs until the next UTC day. Set {MAX_BYTES_ENV}=0 \
                 only for an intentional unbounded capture."
            ),
            Notice::Reopened {
                dropped_lines,
                dropped_bytes,
            } => format!(
                "[logging] daily file budget reopened; the previous UTC day dropped \
                 {dropped_lines} line(s) / {dropped_bytes} bytes"
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Write,
    Drop,
}

#[derive(Debug)]
struct DayBudget {
    limit: Option<u64>,
    day: i32,
    written: u64,
    dropped_lines: u64,
    dropped_bytes: u64,
    exhausted: bool,
}

impl DayBudget {
    fn resuming(limit: Option<u64>, day: i32, written: u64) -> Self {
        Self {
            limit,
            day,
            written,
            dropped_lines: 0,
            dropped_bytes: 0,
            exhausted: false,
        }
    }

    fn admit(&mut self, day: i32, length: usize) -> (Verdict, Vec<Notice>) {
        let Some(limit) = self.limit else {
            return (Verdict::Write, Vec::new());
        };
        let mut notices = self.rollover(day);
        let length = length as u64;
        if self.exhausted || self.written.saturating_add(length) > limit {
            self.dropped_lines = self.dropped_lines.saturating_add(1);
            self.dropped_bytes = self.dropped_bytes.saturating_add(length);
            if !self.exhausted {
                self.exhausted = true;
                notices.push(Notice::Exhausted {
                    limit,
                    written: self.written,
                });
            }
            return (Verdict::Drop, notices);
        }
        self.written = self.written.saturating_add(length);
        (Verdict::Write, notices)
    }

    fn rollover(&mut self, day: i32) -> Vec<Notice> {
        if self.day == day {
            return Vec::new();
        }
        let notice = (self.dropped_lines > 0).then_some(Notice::Reopened {
            dropped_lines: self.dropped_lines,
            dropped_bytes: self.dropped_bytes,
        });
        self.day = day;
        self.written = 0;
        self.dropped_lines = 0;
        self.dropped_bytes = 0;
        self.exhausted = false;
        notice.into_iter().collect()
    }

    fn record_essential(&mut self, day: i32, length: usize) -> Vec<Notice> {
        let Some(limit) = self.limit else {
            return Vec::new();
        };
        let mut notices = self.rollover(day);
        self.written = self.written.saturating_add(length as u64);
        if self.written > limit && !self.exhausted {
            self.exhausted = true;
            notices.push(Notice::Exhausted {
                limit,
                written: self.written,
            });
        }
        notices
    }
}

static SHARED_BUDGET: OnceLock<Arc<Mutex<DayBudget>>> = OnceLock::new();

fn shared_budget(limit: Option<u64>, day: i32, written: u64) -> Arc<Mutex<DayBudget>> {
    SHARED_BUDGET
        .get_or_init(|| Arc::new(Mutex::new(DayBudget::resuming(limit, day, written))))
        .clone()
}

pub(super) fn record_essential_write(length: usize) {
    let Some(budget) = SHARED_BUDGET.get() else {
        return;
    };
    let notices = budget
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .record_essential(current_unix_day(), length);
    for notice in notices {
        report_notice(notice);
    }
}

fn report_notice(notice: Notice) {
    let message = notice.message();
    eprintln!("{message}");
    let Some(hub) = crate::logging::hub::log_hub() else {
        return;
    };
    hub.record(crate::logging::hub::LogRecord {
        seq: hub.next_seq(),
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0),
        level: "WARN",
        target: "iyw_claw_lib::logging".to_string(),
        message,
        fields: Default::default(),
        spans: Vec::new(),
    });
}

pub(super) struct BudgetedWriter<W> {
    inner: W,
    budget: Arc<Mutex<DayBudget>>,
    pending: Vec<u8>,
    write_failure: Option<io::ErrorKind>,
}

impl<W: Write> BudgetedWriter<W> {
    pub(super) fn resuming(inner: W, limit: Option<u64>, day: i32, already_written: u64) -> Self {
        Self {
            inner,
            budget: shared_budget(limit, day, already_written),
            pending: Vec::new(),
            write_failure: None,
        }
    }

    fn ensure_writable(&self) -> io::Result<()> {
        let Some(kind) = self.write_failure else {
            return Ok(());
        };
        Err(io::Error::new(
            kind,
            "log writer disabled after a previous write failure",
        ))
    }

    fn take_complete_record(&mut self) -> Option<Vec<u8>> {
        let newline = self.pending.iter().position(|byte| *byte == b'\n')?;
        Some(self.pending.drain(..=newline).collect())
    }

    fn admit_record(&self, length: usize) -> Verdict {
        let (verdict, notices) = self
            .budget
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .admit(current_unix_day(), length);
        for notice in notices {
            report_notice(notice);
        }
        verdict
    }

    fn write_complete_records(&mut self) -> io::Result<()> {
        while let Some(record) = self.take_complete_record() {
            if self.admit_record(record.len()) == Verdict::Write {
                if let Err(error) = self.inner.write_all(&record) {
                    self.write_failure = Some(error.kind());
                    return Err(error);
                }
            }
        }
        Ok(())
    }
}

impl<W: Write> Write for BudgetedWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.ensure_writable()?;
        self.pending.extend_from_slice(buffer);
        self.write_complete_records()?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Err(error) = self.inner.flush() {
            self.write_failure = Some(error.kind());
            return Err(error);
        }
        Ok(())
    }
}
