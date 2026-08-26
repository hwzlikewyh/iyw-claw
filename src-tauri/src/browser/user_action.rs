use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::agent_tool_cancellation::{cancelled_error, ensure_request_active, AgentToolContext};
use super::agent_tool_support::{
    default_agent_tab_id, invalid_argument, optional_string, project_agent_state, required_string,
};
use super::control::ControlGate;
use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;
use super::types::BrowserUserActionRequestSnapshot;
use super::user_action_completion::{parse_completion, UserActionCompletion};

const DEFAULT_TIMEOUT_MS: u64 = 180_000;
const MAX_TIMEOUT_MS: u64 = 300_000;
const USER_QUIET_PERIOD: Duration = Duration::from_millis(1_500);
const COMPLETION_POLL_PERIOD: Duration = Duration::from_millis(1_500);

#[derive(Debug)]
pub(super) struct PendingUserAction {
    pub snapshot: BrowserUserActionRequestSnapshot,
    pub cancellation: CancellationToken,
}

impl BrowserSessionManager {
    pub(super) async fn agent_request_user_action(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        ensure_request_active(context)?;
        let tab_id = optional_string(input, "tab_id", 128)?.map(str::to_string);
        let reason = required_string(input, "reason", 1_024)?.to_string();
        let completion = parse_completion(input)?;
        let timeout = parse_timeout(input)?;
        let tab_id = match tab_id {
            Some(tab_id) => tab_id,
            None => default_agent_tab_id(&self.agent_snapshot_for(context.identity).await)
                .ok_or_else(|| invalid_argument("No managed browser tab is available"))?,
        };
        self.agent_turn_leases
            .register(context.identity, &tab_id)
            .await?;
        self.agent_turn_leases.keep_tab_open(&tab_id).await;
        let (request_id, gate, initial_activity, request_cancellation) =
            self.begin_user_action_request(&tab_id, reason).await?;
        let result = self
            .wait_for_user_action(
                context,
                &tab_id,
                completion,
                timeout,
                gate,
                initial_activity,
                request_cancellation,
            )
            .await;
        self.finish_user_action_request(&request_id, &tab_id).await;
        result
    }

    async fn wait_for_user_action(
        &self,
        context: AgentToolContext<'_>,
        tab_id: &str,
        completion: Option<UserActionCompletion>,
        timeout: Duration,
        gate: ControlGate,
        mut activity: u64,
        request_cancellation: CancellationToken,
    ) -> Result<Value, BrowserError> {
        let deadline = Instant::now() + timeout;
        activity = self
            .wait_for_activity(context, &gate, activity, &request_cancellation, deadline)
            .await?;
        loop {
            activity = self
                .wait_for_quiet(context, &gate, activity, &request_cancellation, deadline)
                .await?;
            let Some(completion) = completion.as_ref() else {
                self.set_user_held(tab_id, false).await?;
                return self
                    .user_action_state(context, tab_id, "agent_review_required")
                    .await;
            };
            let matched = self
                .evaluate_user_action_completion(context, tab_id, completion)
                .await?;
            if matched {
                self.set_user_held(tab_id, false).await?;
                return self.user_action_state(context, tab_id, "completed").await;
            }
            if Instant::now() >= deadline {
                self.set_user_held(tab_id, false).await?;
                return self.user_action_state(context, tab_id, "timed_out").await;
            }
            self.set_user_held(tab_id, true).await?;
            let activity_wait = gate.wait_for_activity(activity);
            tokio::pin!(activity_wait);
            let completion_poll = tokio::time::sleep(COMPLETION_POLL_PERIOD);
            tokio::pin!(completion_poll);
            tokio::select! {
                _ = context.cancellation.cancelled() => return Err(cancelled_error()),
                _ = request_cancellation.cancelled() => return Err(cancelled_error()),
                next = &mut activity_wait => activity = next?,
                _ = &mut completion_poll => {}
            }
        }
    }

    async fn wait_for_activity(
        &self,
        context: AgentToolContext<'_>,
        gate: &ControlGate,
        activity: u64,
        request_cancellation: &CancellationToken,
        deadline: Instant,
    ) -> Result<u64, BrowserError> {
        let wait = gate.wait_for_activity(activity);
        tokio::pin!(wait);
        tokio::select! {
            _ = context.cancellation.cancelled() => Err(cancelled_error()),
            _ = request_cancellation.cancelled() => Err(cancelled_error()),
            result = tokio::time::timeout_at(deadline.into(), &mut wait) => {
                result.map_err(|_| BrowserError::new(
                    BrowserErrorCode::BrowserOperationTimeout,
                    "The browser user-action request timed out",
                ).retryable(true))?
            }
        }
    }

    async fn wait_for_quiet(
        &self,
        context: AgentToolContext<'_>,
        gate: &ControlGate,
        mut activity: u64,
        request_cancellation: &CancellationToken,
        deadline: Instant,
    ) -> Result<u64, BrowserError> {
        loop {
            let sleep = tokio::time::sleep(USER_QUIET_PERIOD);
            tokio::pin!(sleep);
            tokio::select! {
                _ = context.cancellation.cancelled() => return Err(cancelled_error()),
                _ = request_cancellation.cancelled() => return Err(cancelled_error()),
                _ = &mut sleep => {
                    let current = gate.snapshot().await.activity_sequence;
                    if current == activity { return Ok(current); }
                    activity = current;
                }
                result = gate.wait_for_activity(activity) => {
                    activity = result?;
                }
            }
            if Instant::now() >= deadline {
                return Err(BrowserError::new(
                    BrowserErrorCode::BrowserOperationTimeout,
                    "The browser user-action request timed out",
                )
                .retryable(true));
            }
        }
    }

    async fn user_action_state(
        &self,
        context: AgentToolContext<'_>,
        tab_id: &str,
        status: &str,
    ) -> Result<Value, BrowserError> {
        Ok(project_agent_state(
            self.agent_snapshot_for(context.identity).await,
            Some(tab_id),
            Some(json!({ "status": status })),
        ))
    }

    async fn begin_user_action_request(
        &self,
        tab_id: &str,
        reason: String,
    ) -> Result<(String, ControlGate, u64, CancellationToken), BrowserError> {
        let gate = self
            .control_gate(tab_id)
            .await
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        let mut requests = self.user_action_requests.lock().await;
        if requests
            .values()
            .any(|request| request.snapshot.browser_tab_id == tab_id)
        {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserControlChanged,
                "A browser user-action request is already waiting on this tab",
            )
            .retryable(true));
        }
        let request_id = Uuid::new_v4().to_string();
        let cancellation = CancellationToken::new();
        requests.insert(
            request_id.clone(),
            PendingUserAction {
                snapshot: BrowserUserActionRequestSnapshot {
                    request_id: request_id.clone(),
                    browser_tab_id: tab_id.to_string(),
                    reason,
                },
                cancellation: cancellation.clone(),
            },
        );
        let initial_activity = gate.snapshot().await.activity_sequence;
        drop(requests);
        gate.set_user_held(true).await;
        Ok((request_id, gate, initial_activity, cancellation))
    }

    async fn finish_user_action_request(&self, request_id: &str, tab_id: &str) {
        if let Some(request) = self.user_action_requests.lock().await.remove(request_id) {
            request.cancellation.cancel();
        }
        let _ = self.set_user_held(tab_id, false).await;
    }

    pub(super) async fn cancel_user_action_requests(&self, tab_ids: Vec<String>) {
        let mut requests = self.user_action_requests.lock().await;
        let cancelled = requests
            .keys()
            .filter(|request_id| {
                tab_ids.is_empty()
                    || requests
                        .get(*request_id)
                        .is_some_and(|request| tab_ids.contains(&request.snapshot.browser_tab_id))
            })
            .cloned()
            .collect::<Vec<_>>();
        let removed = cancelled
            .into_iter()
            .filter_map(|request_id| requests.remove(&request_id))
            .collect::<Vec<_>>();
        drop(requests);
        for request in removed {
            request.cancellation.cancel();
            let _ = self
                .set_user_held(&request.snapshot.browser_tab_id, false)
                .await;
        }
    }
}

fn parse_timeout(input: &Value) -> Result<Duration, BrowserError> {
    let value = input
        .get("timeout_ms")
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| invalid_argument("Invalid browser timeout_ms"))
        })
        .transpose()?
        .unwrap_or(DEFAULT_TIMEOUT_MS);
    if !(1_000..=MAX_TIMEOUT_MS).contains(&value) {
        return Err(invalid_argument("Browser timeout_ms is out of range"));
    }
    Ok(Duration::from_millis(value))
}
