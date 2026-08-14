use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sacp::schema::{Error, ErrorCode};

const TOTAL_TIMEOUT: Duration = Duration::from_secs(60);
const RESUME_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy)]
pub enum RecoveryStage {
    Resume,
    Load,
}

#[derive(Debug, Clone, Copy)]
pub struct RecoveryProgressSnapshot {
    pub stage: RecoveryStage,
    pub elapsed_ms: u128,
    pub remaining_ms: u128,
}

#[derive(Debug)]
pub struct RecoveryProgress {
    stage: AtomicU8,
    started_at: Mutex<Option<Instant>>,
}

impl RecoveryProgress {
    pub fn new() -> Self {
        Self {
            stage: AtomicU8::new(0),
            started_at: Mutex::new(None),
        }
    }

    pub fn begin(&self, stage: RecoveryStage) {
        *self.started_at.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        self.set_stage(stage);
    }

    pub fn set_stage(&self, stage: RecoveryStage) {
        let value = match stage {
            RecoveryStage::Resume => 1,
            RecoveryStage::Load => 2,
        };
        self.stage.store(value, Ordering::Release);
    }

    pub fn finish(&self) {
        self.stage.store(0, Ordering::Release);
        *self.started_at.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub fn take(&self) -> Option<RecoveryProgressSnapshot> {
        let stage = match self.stage.swap(0, Ordering::AcqRel) {
            1 => RecoveryStage::Resume,
            2 => RecoveryStage::Load,
            _ => return None,
        };
        let started_at = self
            .started_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()?;
        let elapsed = started_at.elapsed();
        Some(RecoveryProgressSnapshot {
            stage,
            elapsed_ms: elapsed.as_millis(),
            remaining_ms: TOTAL_TIMEOUT.saturating_sub(elapsed).as_millis(),
        })
    }
}

impl RecoveryStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resume => "resume",
            Self::Load => "load",
        }
    }
}

#[derive(Debug)]
pub enum RecoveryFailure {
    Timeout,
    Transport(String),
    InvalidResponse(String),
    Remote(Error),
    Unavailable(String),
}

impl RecoveryFailure {
    pub fn from_request_error(error: Error) -> Self {
        if is_transport_error(&error) {
            Self::Transport(error.to_string())
        } else if is_local_request_error(&error) {
            Self::InvalidResponse(error.to_string())
        } else {
            Self::Remote(error)
        }
    }

    pub fn allows_load_fallback(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Transport(_) => "transport",
            Self::InvalidResponse(_) => "invalid_response",
            Self::Remote(_) => "remote_rpc",
            Self::Unavailable(_) => "unavailable",
        }
    }

    pub fn stable_code(&self) -> &'static str {
        match self {
            Self::Timeout => "session_recovery_timeout",
            Self::Transport(_) => "session_recovery_transport",
            Self::InvalidResponse(_) => "session_recovery_invalid_response",
            Self::Remote(error) => classify_remote_error(error),
            Self::Unavailable(_) => "session_unavailable",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Timeout => "Session recovery timed out".to_string(),
            Self::Transport(message)
            | Self::InvalidResponse(message)
            | Self::Unavailable(message) => message.clone(),
            Self::Remote(error) => error.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct RecoveryBudget {
    started_at: Instant,
}

impl RecoveryBudget {
    pub fn start() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }

    pub fn timeout_for(&self, stage: RecoveryStage) -> Option<Duration> {
        let remaining = TOTAL_TIMEOUT.checked_sub(self.started_at.elapsed())?;
        let timeout = match stage {
            RecoveryStage::Resume => remaining.min(RESUME_TIMEOUT),
            RecoveryStage::Load => remaining,
        };
        (!timeout.is_zero()).then_some(timeout)
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.started_at.elapsed().as_millis()
    }

    pub fn remaining_ms(&self) -> u128 {
        TOTAL_TIMEOUT
            .saturating_sub(self.started_at.elapsed())
            .as_millis()
    }
}

fn classify_remote_error(error: &Error) -> &'static str {
    if matches!(error.code, ErrorCode::ResourceNotFound) {
        "resource_not_found"
    } else {
        "session_unavailable"
    }
}

fn is_transport_error(error: &Error) -> bool {
    if !matches!(error.code, ErrorCode::InternalError) {
        return false;
    }
    error
        .data
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .is_some_and(|data| {
            (data.starts_with("response to `") && data.contains("never received:"))
                || data.starts_with("failed to send outgoing request `")
        })
}

fn is_local_request_error(error: &Error) -> bool {
    matches!(error.code, ErrorCode::InternalError)
        && error
            .data
            .as_ref()
            .and_then(serde_json::Value::as_str)
            .is_some_and(|data| data.starts_with("failed to create untyped request for `"))
}
