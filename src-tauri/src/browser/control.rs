use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::control_lease::AgentControlLease;
use super::control_waiter::QueuedAgentWaiter;
use super::error::{BrowserError, BrowserErrorCode};
use super::types::BrowserControlStatus;

mod user;

const USER_ACTIVE_LEASE: Duration = Duration::from_millis(1_500);
const AGENT_WAIT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub struct ControlGate {
    inner: Arc<Mutex<ControlInner>>,
    notify: Arc<Notify>,
}

#[derive(Debug)]
struct ControlInner {
    epoch: u64,
    held: bool,
    closed: bool,
    agent_enabled: bool,
    user_active_until: Option<Instant>,
    active_user_operations: usize,
    activity_sequence: u64,
    active_agent: Option<ActiveAgent>,
    queue: VecDeque<String>,
}

#[derive(Debug)]
struct ActiveAgent {
    operation_id: String,
    cancellation: CancellationToken,
}

#[derive(Debug, Clone, Copy)]
pub struct ControlSnapshot {
    pub status: BrowserControlStatus,
    pub epoch: u64,
    pub waiting_agents: usize,
}

impl ControlGate {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ControlInner {
                epoch: 0,
                held: false,
                closed: false,
                agent_enabled: true,
                user_active_until: None,
                active_user_operations: 0,
                activity_sequence: 0,
                active_agent: None,
                queue: VecDeque::new(),
            })),
            notify: Arc::new(Notify::new()),
        }
    }

    pub async fn snapshot(&self) -> ControlSnapshot {
        let mut inner = self.inner.lock().await;
        expire_user_activity(&mut inner);
        ControlSnapshot {
            status: control_status(&inner),
            epoch: inner.epoch,
            waiting_agents: inner.queue.len(),
        }
    }

    pub async fn record_user_input(&self, semantic: bool) -> ControlSnapshot {
        let sequence = {
            let mut inner = self.inner.lock().await;
            if semantic {
                inner.epoch = inner.epoch.saturating_add(1);
                if let Some(active) = &inner.active_agent {
                    active.cancellation.cancel();
                }
            }
            inner.activity_sequence = inner.activity_sequence.saturating_add(1);
            inner.user_active_until = Some(Instant::now() + USER_ACTIVE_LEASE);
            inner.activity_sequence
        };
        self.notify.notify_waiters();
        self.schedule_user_idle(sequence);
        self.snapshot().await
    }

    pub async fn set_user_held(&self, held: bool) -> ControlSnapshot {
        {
            let mut inner = self.inner.lock().await;
            if inner.held != held {
                inner.epoch = inner.epoch.saturating_add(1);
            }
            inner.held = held;
            inner.user_active_until = None;
            if held {
                if let Some(active) = &inner.active_agent {
                    active.cancellation.cancel();
                }
            }
        }
        self.notify.notify_waiters();
        self.snapshot().await
    }

    pub async fn acquire_agent(&self) -> Result<AgentControlLease, BrowserError> {
        let operation_id = Uuid::new_v4().to_string();
        let mut waiter = QueuedAgentWaiter::new(self.clone(), operation_id.clone());
        let deadline = Instant::now() + AGENT_WAIT_TIMEOUT;
        loop {
            let notified = self.notify.notified();
            let decision = {
                let mut inner = self.inner.lock().await;
                expire_user_activity(&mut inner);
                acquire_decision(&mut inner, &operation_id)
            };
            match decision {
                AcquireDecision::Acquired {
                    epoch,
                    cancellation,
                } => {
                    waiter.disarm();
                    return Ok(AgentControlLease::new(
                        self.clone(),
                        operation_id,
                        epoch,
                        cancellation,
                    ));
                }
                AcquireDecision::Error(error) => return Err(error),
                AcquireDecision::Wait => {}
            }
            let now = Instant::now();
            if now >= deadline {
                self.remove_waiter(&operation_id).await;
                waiter.disarm();
                return Err(BrowserError::new(
                    BrowserErrorCode::BrowserUserActive,
                    "The user is actively controlling this browser tab",
                )
                .retryable(true));
            }
            let _ = tokio::time::timeout(deadline - now, notified).await;
        }
    }

    pub async fn close(&self) {
        let mut inner = self.inner.lock().await;
        inner.closed = true;
        inner.epoch = inner.epoch.saturating_add(1);
        if let Some(active) = &inner.active_agent {
            active.cancellation.cancel();
        }
        inner.queue.clear();
        drop(inner);
        self.notify.notify_waiters();
    }

    fn schedule_user_idle(&self, sequence: u64) {
        let gate = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(USER_ACTIVE_LEASE).await;
            let mut inner = gate.inner.lock().await;
            if inner.activity_sequence == sequence {
                expire_user_activity(&mut inner);
            }
            drop(inner);
            gate.notify.notify_waiters();
        });
    }

    pub(super) async fn remove_waiter(&self, operation_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.queue.retain(|id| id != operation_id);
        drop(inner);
        self.notify.notify_waiters();
    }

    pub(super) async fn complete_agent(&self, operation_id: &str) {
        let mut inner = self.inner.lock().await;
        if inner
            .active_agent
            .as_ref()
            .is_some_and(|active| active.operation_id == operation_id)
        {
            inner.active_agent = None;
        }
        drop(inner);
        self.notify.notify_waiters();
    }

    pub(super) async fn complete_user(&self) {
        let mut inner = self.inner.lock().await;
        inner.active_user_operations = inner.active_user_operations.saturating_sub(1);
        drop(inner);
        self.notify.notify_waiters();
    }
}

enum AcquireDecision {
    Acquired {
        epoch: u64,
        cancellation: CancellationToken,
    },
    Wait,
    Error(BrowserError),
}

fn acquire_decision(inner: &mut ControlInner, operation_id: &str) -> AcquireDecision {
    if inner.closed {
        return AcquireDecision::Error(BrowserError::new(
            BrowserErrorCode::BrowserTabGone,
            "The browser tab is closed",
        ));
    }
    if !inner.agent_enabled {
        inner.queue.retain(|id| id != operation_id);
        return AcquireDecision::Error(BrowserError::new(
            BrowserErrorCode::BrowserTabAccessDenied,
            "Agent access to this browser tab is disabled",
        ));
    }
    if inner.held {
        inner.queue.retain(|id| id != operation_id);
        return AcquireDecision::Error(BrowserError::new(
            BrowserErrorCode::BrowserUserHeld,
            "The user has taken control of this browser tab",
        ));
    }
    if !inner.queue.iter().any(|id| id == operation_id) {
        inner.queue.push_back(operation_id.to_string());
    }
    if inner.active_user_operations == 0
        && inner.user_active_until.is_none()
        && inner.active_agent.is_none()
        && inner.queue.front().is_some_and(|id| id == operation_id)
    {
        inner.queue.pop_front();
        let cancellation = CancellationToken::new();
        inner.active_agent = Some(ActiveAgent {
            operation_id: operation_id.to_string(),
            cancellation: cancellation.clone(),
        });
        return AcquireDecision::Acquired {
            epoch: inner.epoch,
            cancellation,
        };
    }
    AcquireDecision::Wait
}

fn expire_user_activity(inner: &mut ControlInner) {
    if inner
        .user_active_until
        .is_some_and(|until| until <= Instant::now())
    {
        inner.user_active_until = None;
    }
}

fn control_status(inner: &ControlInner) -> BrowserControlStatus {
    if inner.held {
        BrowserControlStatus::UserHeld
    } else if inner.active_user_operations > 0 {
        BrowserControlStatus::UserActive
    } else if inner.user_active_until.is_some() && !inner.queue.is_empty() {
        BrowserControlStatus::AgentWaiting
    } else if inner.user_active_until.is_some() {
        BrowserControlStatus::UserActive
    } else if inner.active_agent.is_some() {
        BrowserControlStatus::AgentRunning
    } else if !inner.queue.is_empty() {
        BrowserControlStatus::AgentWaiting
    } else {
        BrowserControlStatus::Idle
    }
}
