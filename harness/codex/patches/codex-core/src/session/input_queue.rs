use crate::agent::api::AgentControl;
use crate::agent_communication::PENDING_MAILBOX_MESSAGES;
use crate::state::ActiveTurn;
use crate::state::MailboxDeliveryPhase;
use crate::state::TurnState;
use codex_diagnostics::GaugeGuard;
use codex_history::ResponseItemEnvelope;
use codex_protocol::ThreadId;
use codex_protocol::models::ResponseItem;
use codex_protocol::turn_input::TurnStartOptions;
use codex_user_input::ResolvedUserInput;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;

/// Host capture metadata belonging to one input, including steers within another turn.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInputMetadata {
    /// Private extension data carried with this queued input through completion.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extension_data: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance_order: Option<u64>,
    /// Optional host-assigned UUID for this particular Responses user item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_item_uuid: Option<uuid::Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_dictation: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "codex_history::UserInputOrigin::is_user"
    )]
    pub origin: codex_history::UserInputOrigin,
}

/// Input consumed by a regular turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TurnInput {
    UserInput {
        content: Vec<ResolvedUserInput>,
        client_id: Option<String>,
        #[serde(flatten)]
        metadata: UserInputMetadata,
    },
    FunctionCallOutput(#[serde(with = "turn_input_response_item")] ResponseItemEnvelope),
    // Preserve the existing serialized format while carrying injection API metadata
    // through the in-memory queue.
    ResponseItem(#[serde(with = "turn_input_response_item")] ResponseItemEnvelope),
    InterAgentCommunication(crate::PendingAgentMessage),
}

mod turn_input_response_item {
    use super::ResponseItem;
    use super::ResponseItemEnvelope;
    use serde::Deserialize;
    use serde::Deserializer;
    use serde::Serialize;
    use serde::Serializer;
    use serde::ser::Error as _;

    pub(super) fn serialize<S>(
        item: &ResponseItemEnvelope,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if item.metadata.is_some() {
            return Err(S::Error::custom(
                "annotated response items cannot cross the turn-input serialization boundary",
            ));
        }
        item.item.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<ResponseItemEnvelope, D::Error>
    where
        D: Deserializer<'de>,
    {
        ResponseItem::deserialize(deserializer).map(ResponseItemEnvelope::new)
    }
}

/// Steering takes precedence when several kinds of input are pending together.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum InputQueueActivity {
    Mailbox,
    Steer,
}

impl TurnInput {
    fn is_queue_only_mail(&self) -> bool {
        match self {
            Self::InterAgentCommunication(communication) => !communication.trigger_turn,
            Self::ResponseItem(ResponseItemEnvelope {
                item: ResponseItem::AgentMessage { .. },
                ..
            }) => true,
            Self::UserInput { .. } | Self::FunctionCallOutput(_) | Self::ResponseItem(_) => false,
        }
    }

    fn wait_activity(&self) -> InputQueueActivity {
        match self {
            Self::InterAgentCommunication(_)
            | Self::ResponseItem(ResponseItemEnvelope {
                item: ResponseItem::AgentMessage { .. },
                ..
            }) => InputQueueActivity::Mailbox,
            Self::UserInput { .. } | Self::FunctionCallOutput(_) | Self::ResponseItem(_) => {
                InputQueueActivity::Steer
            }
        }
    }
}

/// Turn-local pending input storage owned by the input queue flow.
#[derive(Default)]
pub(crate) struct TurnInputQueue {
    items: Vec<TurnInput>,
    /// Activity from notifying writes. Passive response items do not wake a wait.
    notified_activity: Option<InputQueueActivity>,
}

/// Session-scoped pending input storage and active-turn mailbox delivery coordination.
pub(crate) struct InputQueue {
    activity_tx: watch::Sender<InputQueueActivity>,
    controller: Option<(ThreadId, Arc<dyn AgentControl>, watch::Receiver<bool>)>,
    mailbox_pending_mails: Mutex<VecDeque<PendingMailboxCommunication>>,
}

pub(crate) struct PendingMailboxCommunication {
    pub(crate) communication: crate::PendingAgentMessage,
    start_options: TurnStartOptions,
    _diagnostics_guard: GaugeGuard,
}

impl InputQueue {
    pub(crate) fn new() -> Self {
        let (activity_tx, _) = watch::channel(InputQueueActivity::Mailbox);
        Self {
            activity_tx,
            controller: None,
            mailbox_pending_mails: Mutex::new(VecDeque::new()),
        }
    }

    pub(crate) fn with_controller(thread_id: ThreadId, control: Arc<dyn AgentControl>) -> Self {
        let updates = control.watch_mailbox(thread_id);
        Self {
            controller: Some((thread_id, control, updates)),
            ..Self::new()
        }
    }

    pub(crate) fn mailbox_updates(&self) -> Option<watch::Receiver<bool>> {
        self.controller
            .as_ref()
            .map(|(_, _, updates)| updates.clone())
    }

    pub(crate) fn notify_mailbox(&self) {
        self.activity_tx.send_replace(InputQueueActivity::Mailbox);
    }

    fn read_mailbox(&self, pending: &mut VecDeque<PendingMailboxCommunication>) {
        if let Some((thread_id, control, _)) = &self.controller {
            pending.extend(
                control
                    .take_mailbox(*thread_id)
                    .into_iter()
                    .map(|communication| PendingMailboxCommunication {
                        communication,
                        start_options: TurnStartOptions::default(),
                        _diagnostics_guard: PENDING_MAILBOX_MESSAGES.track(),
                    }),
            );
        }
    }

    pub(crate) async fn subscribe_activity(
        &self,
        turn_state: Option<&Mutex<TurnState>>,
    ) -> (
        watch::Receiver<InputQueueActivity>,
        Option<InputQueueActivity>,
    ) {
        let activity_rx = self.activity_tx.subscribe();
        let turn_activity = if let Some(turn_state) = turn_state {
            turn_state.lock().await.pending_input.pending_activity()
        } else {
            None
        };
        let pending_activity = if let Some(activity) = turn_activity {
            Some(activity)
        } else if self.has_pending_mailbox_items().await {
            Some(InputQueueActivity::Mailbox)
        } else {
            None
        };
        (activity_rx, pending_activity)
    }

    pub(crate) async fn enqueue_mailbox_communication(
        &self,
        communication: crate::PendingAgentMessage,
        start_options: TurnStartOptions,
    ) {
        let mut pending = self.mailbox_pending_mails.lock().await;
        // Mail retained while unloaded precedes new submissions to the loaded session.
        self.read_mailbox(&mut pending);
        pending.push_back(PendingMailboxCommunication {
            communication,
            start_options,
            _diagnostics_guard: PENDING_MAILBOX_MESSAGES.track(),
        });
        self.activity_tx.send_replace(InputQueueActivity::Mailbox);
    }

    pub(crate) async fn has_pending_mailbox_items(&self) -> bool {
        !self.mailbox_pending_mails.lock().await.is_empty()
            || self
                .controller
                .as_ref()
                .is_some_and(|(_, _, updates)| *updates.borrow())
    }

    /// Native mailbox backlog qualifies regardless of turn ID or delivery phase.
    /// Injected mail qualifies only while its matching turn still accepts mail.
    pub(crate) async fn should_preempt_sampling_for_mail(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
    ) -> bool {
        if self.has_pending_mailbox_items().await {
            return true;
        }
        let Some(turn_state) = self.turn_state_for_sub_id(active_turn, sub_id).await else {
            return false;
        };
        let turn_state = turn_state.lock().await;
        turn_state.accepts_mailbox_delivery_for_current_turn()
            && turn_state.pending_input.items.iter().any(|input| {
                matches!(
                    input,
                    TurnInput::ResponseItem(ResponseItemEnvelope {
                        item: ResponseItem::AgentMessage { .. },
                        ..
                    })
                )
            })
    }

    pub(crate) async fn has_trigger_turn_mailbox_items(&self) -> bool {
        self.mailbox_pending_mails
            .lock()
            .await
            .iter()
            .any(|mail| mail.communication.trigger_turn)
    }

    pub(crate) async fn drain_mailbox(&self) -> Vec<PendingMailboxCommunication> {
        let mut pending = self.mailbox_pending_mails.lock().await;
        self.read_mailbox(&mut pending);
        pending.drain(..).collect()
    }

    pub(crate) async fn pending_wake_options(&self) -> Option<TurnStartOptions> {
        let mut mails = self.mailbox_pending_mails.lock().await;
        self.read_mailbox(&mut mails);
        let mut options = Self::trigger_turn_options(&mails)?;
        if let Some(mail) = mails.iter().find(|mail| mail.communication.trigger_turn) {
            options.initiating_agent_path = Some(mail.communication.author.clone());
        }
        Some(options)
    }

    fn trigger_turn_options(
        pending_mails: &VecDeque<PendingMailboxCommunication>,
    ) -> Option<TurnStartOptions> {
        // A later follow-up supersedes the earlier choice, including an omitted choice.
        let mut start_options = pending_mails
            .iter()
            .rev()
            .find(|mail| mail.communication.trigger_turn)
            .map(|mail| mail.start_options.clone())?;
        start_options.parent_turn_id = pending_mails
            .iter()
            .filter(|mail| mail.communication.trigger_turn)
            .map(|mail| mail.start_options.parent_turn_id.as_deref())
            .reduce(|expected, candidate| expected.filter(|id| candidate == Some(*id)))
            .and_then(|id| id.filter(|id| !id.trim().is_empty()).map(str::to_string));
        start_options.root_turn_id = pending_mails
            .iter()
            .find(|mail| mail.communication.trigger_turn)
            .and_then(|mail| {
                mail.start_options
                    .parent_turn_id
                    .as_deref()
                    .filter(|id| !id.trim().is_empty())
                    .and(mail.start_options.root_turn_id.as_deref())
                    .filter(|id| !id.trim().is_empty())
            })
            .map(str::to_string);
        Some(start_options)
    }

    pub(crate) async fn drain_mailbox_input_items(&self) -> (Vec<TurnInput>, TurnStartOptions) {
        let mut pending_mails = self.mailbox_pending_mails.lock().await;
        self.read_mailbox(&mut pending_mails);
        let start_options = Self::trigger_turn_options(&pending_mails).unwrap_or_default();
        let items = pending_mails
            .drain(..)
            .map(|mail| TurnInput::InterAgentCommunication(mail.communication))
            .collect();
        (items, start_options)
    }

    pub(crate) async fn turn_state_for_sub_id(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
    ) -> Option<Arc<Mutex<TurnState>>> {
        let active = active_turn.lock().await;
        active.as_ref().and_then(|active_turn| {
            active_turn
                .task
                .as_ref()
                .is_some_and(|task| task.turn_context.sub_id == sub_id)
                .then(|| Arc::clone(&active_turn.turn_state))
        })
    }

    /// Signal once a user message is queued for this sampling request.
    pub(crate) async fn watch_user_input(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
        interrupt: CancellationToken,
    ) -> Option<AbortOnDropHandle<()>> {
        let turn_state = self.turn_state_for_sub_id(active_turn, sub_id).await?;
        // Subscribe before inspecting the queue so an arrival cannot be missed.
        let mut activity = self.activity_tx.subscribe();
        Some(AbortOnDropHandle::new(tokio::spawn(async move {
            loop {
                if turn_state.lock().await.pending_input.has_user_input() {
                    interrupt.cancel();
                    return;
                }
                if activity.changed().await.is_err() {
                    return;
                }
            }
        })))
    }

    /// Clear any pending waiters and input buffered for the current turn.
    pub(crate) async fn clear_pending(&self, active_turn: &ActiveTurn) {
        // Pending mail can own extension attachments whose destructors need the lock released.
        let pending_input = {
            let mut turn_state = active_turn.turn_state.lock().await;
            turn_state.clear_pending_waiters();
            std::mem::take(&mut turn_state.pending_input)
        };
        // A terminal result still in the queue has no writer yet. Keep it available
        // to the next turn or to the source writer during a graceful drain.
        let results: Vec<_> = pending_input
            .items
            .into_iter()
            .filter_map(|input| match input {
                TurnInput::InterAgentCommunication(message) if message.is_terminal_result() => {
                    Some(message)
                }
                _ => None,
            })
            .collect();
        if !results.is_empty() {
            let mut pending = self.mailbox_pending_mails.lock().await;
            for communication in results.into_iter().rev() {
                pending.push_front(PendingMailboxCommunication {
                    communication,
                    start_options: TurnStartOptions::default(),
                    _diagnostics_guard: PENDING_MAILBOX_MESSAGES.track(),
                });
            }
        }
    }

    pub(crate) async fn defer_mailbox_delivery_to_next_turn(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
    ) {
        let turn_state = self.turn_state_for_sub_id(active_turn, sub_id).await;
        let Some(turn_state) = turn_state else {
            return;
        };
        let mut turn_state = turn_state.lock().await;
        // Explicit same-turn work still needs a follow-up. Queue-only agent mail does not: keep
        // it pending so task completion records it for the next turn without sampling again.
        if turn_state
            .pending_input
            .items
            .iter()
            .any(|input| !input.is_queue_only_mail())
        {
            return;
        }
        turn_state.set_mailbox_delivery_phase(MailboxDeliveryPhase::NextTurn);
    }

    pub(crate) async fn accept_mailbox_delivery_for_current_turn(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
    ) {
        let turn_state = self.turn_state_for_sub_id(active_turn, sub_id).await;
        let Some(turn_state) = turn_state else {
            return;
        };
        self.accept_mailbox_delivery_for_turn_state(turn_state.as_ref())
            .await;
    }

    pub(super) async fn accept_mailbox_delivery_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
    ) {
        turn_state
            .lock()
            .await
            .accept_mailbox_delivery_for_current_turn();
    }

    pub(super) async fn extend_pending_input_and_notify_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
        input: Vec<TurnInput>,
    ) {
        let activity = input.iter().map(TurnInput::wait_activity).max();
        let queue_only_mail = !input.is_empty() && input.iter().all(TurnInput::is_queue_only_mail);
        {
            let mut turn_state = turn_state.lock().await;
            turn_state.pending_input.notified_activity =
                turn_state.pending_input.notified_activity.max(activity);
            turn_state.pending_input.items.extend(input);
            if !queue_only_mail {
                turn_state.accept_mailbox_delivery_for_current_turn();
            }
        }
        self.activity_tx
            .send_replace(activity.unwrap_or(InputQueueActivity::Steer));
    }

    pub(crate) async fn extend_pending_input_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
        input: Vec<TurnInput>,
    ) {
        turn_state.lock().await.pending_input.items.extend(input);
    }

    pub(crate) async fn take_pending_input_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
    ) -> Vec<TurnInput> {
        std::mem::take(&mut turn_state.lock().await.pending_input).items
    }

    #[expect(
        clippy::await_holding_invalid_type,
        reason = "active turn checks and turn state updates must remain atomic"
    )]
    pub(crate) async fn get_pending_input(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
    ) -> (Vec<TurnInput>, TurnStartOptions) {
        let (pending_input, accepts_mailbox_delivery) = {
            let mut active = active_turn.lock().await;
            match active.as_mut() {
                Some(active_turn) => {
                    let mut turn_state = active_turn.turn_state.lock().await;
                    let accepts_mailbox_delivery =
                        turn_state.accepts_mailbox_delivery_for_current_turn();
                    let pending_input = if accepts_mailbox_delivery {
                        std::mem::take(&mut turn_state.pending_input).items
                    } else {
                        Vec::new()
                    };
                    (pending_input, accepts_mailbox_delivery)
                }
                None => (Vec::new(), true),
            }
        };
        if !accepts_mailbox_delivery {
            return (pending_input, TurnStartOptions::default());
        }
        let (mailbox_items, start_options) = self.drain_mailbox_input_items().await;
        if pending_input.is_empty() {
            (mailbox_items, start_options)
        } else {
            let mut pending_input = pending_input;
            pending_input.extend(mailbox_items);
            (pending_input, start_options)
        }
    }

    #[expect(
        clippy::await_holding_invalid_type,
        reason = "active turn checks and turn state reads must remain atomic"
    )]
    pub(crate) async fn has_pending_input(&self, active_turn: &Mutex<Option<ActiveTurn>>) -> bool {
        let (has_turn_pending_input, accepts_mailbox_delivery) = {
            let active = active_turn.lock().await;
            match active.as_ref() {
                Some(active_turn) => {
                    let turn_state = active_turn.turn_state.lock().await;
                    (
                        !turn_state.pending_input.is_empty(),
                        turn_state.accepts_mailbox_delivery_for_current_turn(),
                    )
                }
                None => (false, true),
            }
        };
        if !accepts_mailbox_delivery {
            return false;
        }
        if has_turn_pending_input {
            return true;
        }
        self.has_pending_mailbox_items().await
    }
}

impl TurnInputQueue {
    pub(crate) fn has_user_input(&self) -> bool {
        self.items
            .iter()
            .any(|input| matches!(input, TurnInput::UserInput { .. }))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn pending_activity(&self) -> Option<InputQueueActivity> {
        if self.notified_activity == Some(InputQueueActivity::Steer) {
            return self.notified_activity;
        }
        self.items
            .iter()
            .filter(|input| !matches!(input, TurnInput::ResponseItem(_)))
            .map(TurnInput::wait_activity)
            .chain(self.notified_activity)
            .max()
    }
}
