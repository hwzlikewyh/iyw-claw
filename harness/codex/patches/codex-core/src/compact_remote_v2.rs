use codex_login::AuthOwner;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::OnceLock;

use crate::compact_retained_tool_outputs::is_retained_tool_output;
use crate::session::FileAudioDurations;

use crate::Prompt;
use crate::client::ModelClientSession;
use crate::client::ResponsesHttpExecutor;
use crate::client::SamplingOptions;
use crate::client_common::ResponseEvent;
use crate::client_common::SamplingEvent;
use crate::client_common::SamplingStream;
use crate::client_common::SamplingUsage;
use crate::compact::CompactedHistoryMetadata;
use crate::compact::CompactionAnalyticsAttempt;
use crate::compact::CompactionAnalyticsDetails;
use crate::compact::compaction_status_from_result;
use crate::compact::request_budget::RequestBudget;
use crate::compact_model_fallback::record_model_fallback;
use crate::compact_model_fallback::should_retry_with_current_model;
use crate::compact_pending_input::MAX_RETAINED_AGENT_MESSAGE_TOKENS;
use crate::compact_pending_input::is_retained_pending_mail;
use crate::compact_remote_history::newest_history_item_groups;
use crate::context::world_state::WorldState;
use crate::context_manager::estimate_item_token_count;
use crate::context_manager::estimate_item_token_count_with_audio;
use crate::hook_runtime::PostCompactHookOutcome;
use crate::hook_runtime::PreCompactHookOutcome;
use crate::hook_runtime::run_post_compact_hooks;
use crate::hook_runtime::run_pre_compact_hooks;
use crate::responses_metadata::CodexResponsesMetadata;
use crate::responses_metadata::CompactionTurnMetadata;
use crate::responses_retry::ResponsesStreamRequest;
use crate::responses_retry::ResponsesStreamRetryState;
use crate::responses_retry::handle_response_stream_error;
use crate::session::RequestEffortUsage;
use crate::session::SamplingInterrupt;
use crate::session::session::Session;
use crate::session::step_context::StepContext;
use crate::session::turn_context::TurnContext;
use codex_analytics::CompactionImplementation;
use codex_analytics::CompactionPhase;
use codex_analytics::CompactionReason;
use codex_analytics::CompactionTrigger;
use codex_async_utils::OrCancelExt;
use codex_context_fragments::set_annotated_content;
use codex_context_fragments::to_annotated_content;
use codex_features::Feature;
use codex_features::Features;
use codex_history::CodexHarnessMetadata;
use codex_history::ResponseItemEnvelope;
use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrorDetails;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::items::ContextCompactionItem;
use codex_protocol::items::TurnItem;
use codex_protocol::models::AgentMessageInputContent;
use codex_protocol::models::AudioReference;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::TokenUsage;
use codex_protocol::protocol::TruncationPolicy;
use codex_rollout_trace::InferenceTraceContext;
use codex_utils_output_truncation::approx_token_count;
use codex_utils_output_truncation::truncate_text;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

#[path = "compact_remote_v2_attempt.rs"]
mod attempt;
pub(crate) use attempt::BackgroundCompactionCandidate;
use attempt::RemoteCompactV2Attempt;
use attempt::RemoteCompactV2AttemptInput;
pub(crate) use attempt::capture_background_input;
pub(crate) use attempt::run_background_compaction;
use attempt::run_remote_compact_v2_attempt;

#[path = "compact_remote_v2_installation.rs"]
mod installation;
use installation::run_compaction;

#[path = "compact_remote_v2_media.rs"]
mod media;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RetainedImageBudget {
    Disabled,
    Enabled,
}

pub(crate) const RETAINED_MESSAGE_TOKEN_BUDGET: usize = 64_000;
// Compact attempts can run much longer than normal turns, so keep the per-transport
// retry budget smaller than the general Responses stream retry budget.
const MAX_REMOTE_COMPACTION_V2_STREAM_RETRIES: u64 = 2;

#[derive(Clone, Copy)]
enum CompactionRequestKind<'a> {
    Foreground(&'a CancellationToken),
    Background(&'a CancellationToken),
}

enum CompactionRequestTransport<'a> {
    Foreground(&'a mut ModelClientSession),
    Background(Option<String>, Option<AuthOwner>),
}

/// The caller decides whether a declined prepared result needs a foreground request.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompactionFallback {
    Foreground,
    Skip,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_inline_remote_auto_compact_task(
    sess: Arc<Session>,
    step_context: Arc<StepContext>,
    replacement_step_context: Arc<StepContext>,
    client_session: &mut ModelClientSession,
    world_state: Arc<WorldState>,
    reason: CompactionReason,
    phase: CompactionPhase,
    candidate: Option<Box<BackgroundCompactionCandidate>>,
    fallback: CompactionFallback,
) -> CodexResult<bool> {
    let compaction_metadata = CompactionTurnMetadata::new(
        CompactionTrigger::Auto,
        reason,
        CompactionImplementation::ResponsesCompactionV2,
        phase,
    );
    run_remote_compact_task_inner(
        &sess,
        &step_context,
        &replacement_step_context,
        Some(client_session),
        world_state,
        compaction_metadata,
        candidate,
        fallback,
    )
    .await
}

pub(crate) async fn run_remote_compact_task(
    sess: Arc<Session>,
    step_context: Arc<StepContext>,
    world_state: Arc<WorldState>,
) -> CodexResult<()> {
    let compaction_metadata = CompactionTurnMetadata::new(
        CompactionTrigger::Manual,
        CompactionReason::UserRequested,
        CompactionImplementation::ResponsesCompactionV2,
        CompactionPhase::StandaloneTurn,
    );
    run_remote_compact_task_inner(
        &sess,
        &step_context,
        &step_context,
        /*client_session*/ None,
        world_state,
        compaction_metadata,
        /*candidate*/ None,
        CompactionFallback::Foreground,
    )
    .await
    .map(|_| ())
}

#[allow(clippy::too_many_arguments)]
async fn run_remote_compact_task_inner(
    sess: &Arc<Session>,
    step_context: &Arc<StepContext>,
    replacement_step_context: &Arc<StepContext>,
    client_session: Option<&mut ModelClientSession>,
    world_state: Arc<WorldState>,
    compaction_metadata: CompactionTurnMetadata,
    candidate: Option<Box<BackgroundCompactionCandidate>>,
    fallback: CompactionFallback,
) -> CodexResult<bool> {
    let turn_context = &step_context.turn;
    let trigger = compaction_metadata.trigger();
    let reason = compaction_metadata.reason();
    let implementation = compaction_metadata.implementation();
    let phase = compaction_metadata.phase();
    let mut analytics_details = CompactionAnalyticsDetails {
        active_context_tokens_before: Some(sess.get_total_token_usage().await),
        ..Default::default()
    };
    let attempt = CompactionAnalyticsAttempt::begin(
        sess.as_ref(),
        turn_context.as_ref(),
        trigger,
        reason,
        implementation,
        phase,
    )
    .await;
    let pre_compact_outcome = run_pre_compact_hooks(sess, turn_context, trigger).await;
    match pre_compact_outcome {
        PreCompactHookOutcome::Continue => {}
        PreCompactHookOutcome::Stopped => {
            let error = CodexErr::TurnAborted;
            attempt
                .track(
                    sess.as_ref(),
                    codex_analytics::CompactionStatus::Interrupted,
                    Some(&error),
                    analytics_details,
                )
                .await;
            return Err(error);
        }
    }
    let started_item = if candidate.is_none() && !sess.defer_compaction_start(turn_context, phase) {
        let item = TurnItem::ContextCompaction(ContextCompactionItem::new());
        sess.emit_turn_item_started(turn_context, &item).await;
        Some(item)
    } else {
        None
    };
    let candidate = if candidate.is_none() && fallback == CompactionFallback::Foreground {
        crate::session::async_compaction::wait_for_pending(sess, step_context).await
    } else {
        candidate
    };
    // Box the combined request/install future to keep it off the turn stack.
    let result = Box::pin(run_compaction(
        sess,
        step_context,
        replacement_step_context,
        client_session,
        &world_state,
        compaction_metadata,
        &mut analytics_details,
        started_item.as_ref(),
        candidate.map(|candidate| *candidate),
        fallback,
    ))
    .await;
    // Keep the registration through installation, not just through receiving the summary.
    let interrupted = SamplingInterrupt::enabled(turn_context)
        && SamplingInterrupt::unregister(sess, turn_context).await;
    if matches!(result, Ok(false)) && !interrupted {
        if let Some(item) = started_item {
            sess.emit_turn_item_completed(turn_context, item).await;
        }
        return Ok(false);
    }
    let status = compaction_status_from_result(&result);
    let codex_error = result.as_ref().err();
    if result.is_ok() && !interrupted {
        let post_compact_outcome = run_post_compact_hooks(sess, turn_context, trigger).await;
        if let PostCompactHookOutcome::Stopped = post_compact_outcome {
            attempt
                .track(sess.as_ref(), status, codex_error, analytics_details)
                .await;
            return Err(CodexErr::TurnAborted);
        }
    }
    attempt
        .track(sess.as_ref(), status, codex_error, analytics_details)
        .await;
    let result = if interrupted {
        Err(CodexErr::TurnAborted)
    } else {
        result
    };
    let result = sess.finish_compaction(turn_context, phase, result);
    match result {
        Ok(installed) => Ok(installed),
        Err(err)
            if crate::tasks::ends_turn_as_interrupted(&err)
                || matches!(phase, CompactionPhase::PostTurn) =>
        {
            Err(err)
        }
        Err(err) => {
            sess.track_turn_codex_error(turn_context, &err);
            // Pre-turn failures are reported by run_turn after preserving the incoming prompt.
            if !matches!(phase, CompactionPhase::PreTurn) {
                let event = EventMsg::Error(
                    err.to_error_event(Some("Error running remote compact task".to_string())),
                );
                sess.send_event(turn_context, event).await;
            }
            Err(err)
        }
    }
}

struct RemoteCompactionV2Output {
    compaction_output: ResponseItem,
    response_id: String,
    token_usage: Option<TokenUsage>,
}

async fn run_remote_compaction_request_v2(
    request: &mut crate::hook_runtime::sampling::SamplingRequest,
    sess: &Arc<Session>,
    step_context: &Arc<StepContext>,
    prompt: &Arc<Prompt>,
    responses_metadata: &CodexResponsesMetadata,
    transport: &mut CompactionRequestTransport<'_>,
    submitted_audio: Option<&crate::context_manager::rejected_audio::SubmittedFileAudio>,
) -> CodexResult<RemoteCompactionV2Output> {
    let turn_context = &step_context.turn;
    let client_session = match transport {
        CompactionRequestTransport::Background(turn_state, auth_owner) => {
            let settings = &step_context.settings;
            let cancellation = crate::session::async_compaction::shutdown_cancellation(sess).await;
            let effort = sess
                .reasoning_effort_for_request(settings, RequestEffortUsage::Compaction)
                .await;
            let trace = InferenceTraceContext::disabled();
            // Replay the routing token without letting this request change foreground routing.
            let turn_state = Arc::new(turn_state.clone().map(OnceLock::from).unwrap_or_default());
            let http_executor = ResponsesHttpExecutor::new(&sess.services.model_client);
            let send = http_executor.stream_responses_http(
                &turn_state,
                auth_owner.clone(),
                /*event_sender*/ None,
                prompt,
                &settings.model_info,
                &step_context.session_telemetry,
                SamplingOptions {
                    reasoning_effort: effort,
                    reasoning_summary: settings.reasoning_summary,
                    service_tier: settings.service_tier.clone(),
                },
                responses_metadata,
                &trace,
            );
            let stream = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(CodexErr::TurnAborted),
                stream = request.open(sess, step_context, &settings.model_info.slug, &cancellation, send) => stream?,
            };
            return collect_compaction_output(
                sess,
                turn_context,
                stream,
                CompactionRequestKind::Background(&cancellation),
            )
            .await;
        }
        CompactionRequestTransport::Foreground(client_session) => client_session,
    };
    let settings = &turn_context.initial_settings;
    let max_retries = turn_context
        .provider
        .info()
        .stream_max_retries()
        .min(MAX_REMOTE_COMPACTION_V2_STREAM_RETRIES);
    let mut retry_state = ResponsesStreamRetryState::default();
    let cancellation = &step_context.turn_cancellation;
    let interrupt_request = cancellation.child_token();
    let interrupt_at_boundary = SamplingInterrupt::enabled(turn_context);
    loop {
        let trace = InferenceTraceContext::disabled();
        let send = client_session.open_model_response(
            prompt,
            &settings.model_info,
            &turn_context.session_telemetry,
            SamplingOptions {
                reasoning_effort: sess
                    .reasoning_effort_for_request(settings, RequestEffortUsage::Compaction)
                    .await,
                reasoning_summary: settings.reasoning_summary,
                service_tier: step_context.settings.service_tier.clone(),
            },
            responses_metadata,
            &trace,
        );
        let result = match request
            .open(
                sess,
                step_context,
                &settings.model_info.slug,
                cancellation,
                async {
                    if interrupt_at_boundary {
                        SamplingInterrupt::register(sess, turn_context, &interrupt_request).await?;
                    }
                    send.await
                },
            )
            .or_cancel(cancellation)
            .await?
        {
            Ok(stream) => {
                collect_compaction_output(
                    sess,
                    turn_context,
                    stream,
                    CompactionRequestKind::Foreground(cancellation),
                )
                .await
            }
            Err(err) => Err(err),
        };
        if result.is_err()
            && interrupt_at_boundary
            && SamplingInterrupt::unregister(sess, turn_context).await
        {
            return Err(CodexErr::TurnAborted);
        }
        match result {
            Ok(compaction_output) => return Ok(compaction_output),
            Err(err)
                if submitted_audio.is_some_and(|audio| audio.matches_rejection(err.details())) =>
            {
                return Err(err);
            }
            Err(err)
                if crate::responses_retry::is_managed_gateway_timeout(
                    step_context.turn.provider.info().name.as_str(),
                    &err,
                ) =>
            {
                handle_response_stream_error(
                    &mut retry_state,
                    max_retries,
                    err,
                    client_session,
                    sess,
                    step_context,
                    ResponsesStreamRequest::RemoteCompactionV2,
                )
                .await?;
            }
            Err(err)
                if !crate::responses_retry::is_retryable_response_error(
                    step_context.turn.provider.info().name.as_str(),
                    &err,
                ) =>
            {
                return Err(err);
            }
            Err(err) => {
                handle_response_stream_error(
                    &mut retry_state,
                    max_retries,
                    err,
                    client_session,
                    sess,
                    step_context,
                    ResponsesStreamRequest::RemoteCompactionV2,
                )
                .await?;
            }
        }
    }
}

async fn collect_compaction_output(
    sess: &Session,
    turn_context: &TurnContext,
    mut stream: SamplingStream,
    request_kind: CompactionRequestKind<'_>,
) -> CodexResult<RemoteCompactionV2Output> {
    let mut output_item_count = 0usize;
    let mut compaction_count = 0usize;
    let mut compaction_output = None;
    let mut completed_response_id = None;
    let mut completed_token_usage = None;
    loop {
        let event = match request_kind {
            CompactionRequestKind::Foreground(cancellation) => {
                stream.next_for_turn(cancellation).await
            }
            CompactionRequestKind::Background(cancellation) => tokio::select! {
                biased;
                // Charge completed usage even when cancellation arrives at the same time.
                event = stream.next() => event,
                _ = cancellation.cancelled() => return Err(CodexErr::TurnAborted),
            },
        };
        let Some(SamplingEvent { event, hook_output }) = event else {
            break;
        };
        let event = match event {
            Ok(event) => event,
            Err(err) => {
                if let CodexErrorDetails::ResponseIncomplete(incomplete) = err.details() {
                    match request_kind {
                        CompactionRequestKind::Background(_) => {
                            // Match completed background requests without replacing the terminal error.
                            let usage = SamplingUsage {
                                provider: incomplete.token_usage.clone(),
                                charge: hook_output,
                            };
                            let _ = sess
                                .record_rollout_budget_usage(&turn_context.sub_id, &usage)
                                .await;
                        }
                        CompactionRequestKind::Foreground(_) => {
                            sess.record_incomplete_response_usage(
                                turn_context,
                                &turn_context.initial_settings,
                                incomplete,
                                hook_output,
                            )
                            .await;
                            sess.send_token_count_event(turn_context).await;
                        }
                    }
                }
                return Err(err);
            }
        };
        match event {
            ResponseEvent::OutputItemDone(item) => {
                output_item_count += 1;
                if let ResponseItem::Compaction { .. } = item {
                    compaction_count += 1;
                    if compaction_output.is_none() {
                        compaction_output = Some(item);
                    }
                }
            }
            ResponseEvent::Completed {
                response_id,
                token_usage,
                usage_metadata,
                ..
            } => {
                let usage = SamplingUsage {
                    provider: token_usage,
                    charge: hook_output,
                };
                if matches!(request_kind, CompactionRequestKind::Foreground(_)) {
                    sess.record_observed_response_completed(
                        turn_context,
                        &response_id,
                        usage.provider.as_ref(),
                        usage_metadata.as_ref(),
                    )
                    .await;
                }
                // Both callers consumed this response. Whether it contains an
                // installable summary is a separate decision below.
                sess.record_rollout_budget_usage(&turn_context.sub_id, &usage)
                    .await?;
                completed_response_id = Some(response_id);
                completed_token_usage = usage.provider;
                break;
            }
            _ => {}
        }
    }

    let Some(response_id) = completed_response_id else {
        return Err(CodexErr::Stream(
            "remote compaction v2 stream closed before response.completed".to_string(),
        ));
    };

    if compaction_count != 1 {
        return Err(CodexErr::Fatal(format!(
            "remote compaction v2 expected exactly one compaction output item, got {compaction_count} from {output_item_count} output items"
        )));
    }

    let Some(compaction_output) = compaction_output else {
        unreachable!("compaction output must exist when count is exactly one");
    };
    Ok(RemoteCompactionV2Output {
        compaction_output,
        response_id,
        token_usage: completed_token_usage,
    })
}

fn build_v2_compacted_history(
    prompt_input: Vec<ResponseItem>,
    prompt_input_metadata: Vec<Option<CodexHarnessMetadata>>,
    compaction_output: ResponseItem,
    features: &Features,
    image_budget: RetainedImageBudget,
    audio_durations: &FileAudioDurations,
) -> (Vec<ResponseItemEnvelope>, usize) {
    debug_assert_eq!(prompt_input.len(), prompt_input_metadata.len());
    let prompt_input = prompt_input
        .into_iter()
        .zip(prompt_input_metadata)
        .map(|(item, metadata)| ResponseItemEnvelope { item, metadata })
        .collect::<Vec<_>>();
    let no_file_error_sources =
        crate::context_manager::rejected_audio::checkpoint_input_has_no_file_error_sources(
            &prompt_input,
        );
    let pending_sequence = crate::compact_pending_input::pending_mail_sequence(&prompt_input);
    let retained = select_retained_items(
        &prompt_input,
        |item| {
            is_retained_for_remote_compaction_v2(item, features)
                || pending_sequence.is_some_and(|first| is_retained_pending_mail(item, first))
        },
        RETAINED_MESSAGE_TOKEN_BUDGET,
        image_budget,
        audio_durations,
    )
    .into_values()
    .collect::<Vec<_>>();
    let retained_image_count = retained
        .iter()
        .map(|envelope| retained_input_image_count(&envelope.item))
        .sum::<usize>();
    let mut checkpoint = ResponseItemEnvelope::new(compaction_output);
    if no_file_error_sources {
        checkpoint
            .metadata
            .get_or_insert_default()
            .compaction_without_file_error_sources = true;
    }
    let retained =
        crate::compact_pending_input::insert_summary(retained, checkpoint, pending_sequence);
    (retained, retained_image_count)
}

pub(crate) fn retained_client_items(
    items: &[ResponseItemEnvelope],
    audio_durations: &FileAudioDurations,
    should_retain: impl Fn(&ResponseItemEnvelope) -> bool,
) -> BTreeMap<usize, ResponseItemEnvelope> {
    select_retained_items(
        items,
        should_retain,
        RETAINED_MESSAGE_TOKEN_BUDGET,
        RetainedImageBudget::Disabled,
        audio_durations,
    )
}

pub(crate) fn is_client_authored_developer_message(item: &ResponseItemEnvelope) -> bool {
    item.metadata
        .as_ref()
        .is_some_and(|metadata| metadata.client_authored)
        && matches!(&item.item, ResponseItem::Message { role, .. } if role == "developer")
}

pub(crate) fn should_retain_client_item(item: &ResponseItemEnvelope, features: &Features) -> bool {
    if let Some(retain) = item.metadata.as_ref().and_then(|m| m.retain_across_windows) {
        return retain;
    }
    (features.enabled(Feature::RetainClientDeveloperMessages)
        && is_client_authored_developer_message(item))
        || (features.enabled(Feature::RetainClientToolOutputs) && is_retained_tool_output(item))
}

fn is_retained_for_remote_compaction_v2(
    envelope: &ResponseItemEnvelope,
    features: &Features,
) -> bool {
    if should_retain_client_item(envelope, features) {
        return true;
    }
    let item = &envelope.item;
    if let ResponseItem::AgentMessage {
        author,
        recipient,
        content,
        ..
    } = item
    {
        let is_descendant_progress = author
            .strip_prefix(recipient)
            .is_some_and(|suffix| suffix.starts_with('/'))
            && matches!(
                content.first(),
                Some(AgentMessageInputContent::InputText { text })
                    if text.starts_with("Message Type: MESSAGE\n")
                        || text.starts_with("Message Type: CHANNEL_POST\n")
            );
        let is_completion = matches!(
            content.first(),
            Some(AgentMessageInputContent::InputText { text })
                if text.starts_with("Message Type: FINAL_ANSWER\n")
        );
        return !is_descendant_progress
            && !is_completion
            && estimate_item_token_count(item) <= MAX_RETAINED_AGENT_MESSAGE_TOKENS;
    }

    let ResponseItem::Message { role, .. } = item else {
        return false;
    };

    match role.as_str() {
        "user" => matches!(
            crate::event_mapping::parse_turn_item(item),
            Some(TurnItem::UserMessage(_) | TurnItem::HookPrompt(_))
        ),
        _ => false,
    }
}

fn retained_input_image_count(item: &ResponseItem) -> usize {
    if let ResponseItem::FunctionCallOutput { output, .. } = item
        && let FunctionCallOutputBody::ContentItems(content) = &output.body
    {
        return content
            .iter()
            .filter(|item| matches!(item, FunctionCallOutputContentItem::InputImage { .. }))
            .count();
    }
    let ResponseItem::Message { content, .. } = item else {
        return 0;
    };

    content
        .iter()
        .filter(|item| matches!(item, ContentItem::InputImage { .. }))
        .count()
}

pub(crate) fn truncate_retained_messages_for_remote_compaction(
    items: Vec<ResponseItemEnvelope>,
    max_tokens: usize,
    audio_durations: &FileAudioDurations,
) -> Vec<ResponseItemEnvelope> {
    truncate_retained_messages(
        items,
        max_tokens,
        RetainedImageBudget::Disabled,
        audio_durations,
    )
}

fn truncate_retained_messages(
    items: Vec<ResponseItemEnvelope>,
    max_tokens: usize,
    image_budget: RetainedImageBudget,
    audio_durations: &FileAudioDurations,
) -> Vec<ResponseItemEnvelope> {
    select_retained_items(&items, |_| true, max_tokens, image_budget, audio_durations)
        .into_values()
        .collect()
}

/// Select newest first, then return the items at their original positions.
fn select_retained_items(
    items: &[ResponseItemEnvelope],
    should_retain: impl Fn(&ResponseItemEnvelope) -> bool,
    max_tokens: usize,
    image_budget: RetainedImageBudget,
    audio_durations: &FileAudioDurations,
) -> BTreeMap<usize, ResponseItemEnvelope> {
    let mut remaining = max_tokens;
    let mut selected = BTreeMap::new();
    for group in newest_history_item_groups(items) {
        if remaining == 0 {
            break;
        }
        if !should_retain(group.source) {
            continue;
        }

        let charge_full_item =
            group.source.is_always_retained() || is_client_authored_developer_message(group.source);
        let image_budget = if charge_full_item {
            RetainedImageBudget::Disabled
        } else {
            image_budget
        };
        let has_budgeted_image = image_budget == RetainedImageBudget::Enabled
            && retained_input_image_count(&group.source.item) > 0;
        let has_file_audio = matches!(&group.source.item, ResponseItem::Message { content, .. }
        if content.iter().any(|part| matches!(part, ContentItem::InputAudio {
            audio: AudioReference::File { .. }
        })));
        // Charged media is truncated newest-first as a message boundary. Each
        // modality still defines its own grouping and token cost in `media`.
        let requires_media_boundary_truncation = has_budgeted_image || has_file_audio;
        let notice_tokens = group.attached_notice.map_or(0, |notice| {
            retained_message_token_count(
                &notice.item,
                RetainedImageBudget::Disabled,
                audio_durations,
            )
            .max(1)
        });
        // Explicitly retained items and client developer messages also pay for their media.
        let content_tokens =
            retained_message_token_count(&group.source.item, image_budget, audio_durations);
        let source_tokens = if charge_full_item {
            usize::try_from(estimate_item_token_count_with_audio(
                &group.source.item,
                audio_durations,
            ))
            .unwrap_or(usize::MAX)
        } else {
            content_tokens.max(1)
        };
        let token_count = if group.tool_call.is_some() {
            usize::try_from(group.estimated_token_count(audio_durations)).unwrap_or(usize::MAX)
        } else {
            source_tokens.saturating_add(notice_tokens)
        };
        let source = if token_count <= remaining {
            remaining = remaining.saturating_sub(token_count);
            group.source.clone()
        } else if is_retained_tool_output(group.source)
            || (group.source.is_always_retained()
                && !matches!(group.source.item, ResponseItem::Message { .. }))
        {
            // Structured items such as tool results must stay whole.
            break;
        } else if remaining > notice_tokens {
            let available_tokens = remaining - notice_tokens;
            let content_budget = if charge_full_item {
                available_tokens.saturating_sub(source_tokens.saturating_sub(content_tokens))
            } else {
                available_tokens
            };
            if requires_media_boundary_truncation {
                // Oversized media can leave no boundary content. Do not backfill
                // the remaining budget with older messages in that case.
                remaining = 0;
            }
            let truncated_item = if requires_media_boundary_truncation {
                media::truncate_message_to_token_budget(
                    group.source.clone(),
                    content_budget,
                    image_budget,
                    audio_durations,
                )
            } else {
                truncate_message_text_to_token_budget(group.source.clone(), content_budget)
            };
            let Some(mut truncated_item) = truncated_item else {
                continue;
            };
            if charge_full_item {
                let item_tokens = usize::try_from(estimate_item_token_count_with_audio(
                    &truncated_item.item,
                    audio_durations,
                ))
                .unwrap_or(usize::MAX);
                if item_tokens > available_tokens {
                    let adjusted_budget = content_budget
                        .saturating_sub(item_tokens - available_tokens)
                        .saturating_sub(1);
                    let adjusted = if requires_media_boundary_truncation {
                        media::truncate_message_to_token_budget(
                            truncated_item,
                            adjusted_budget,
                            image_budget,
                            audio_durations,
                        )
                    } else {
                        truncate_message_text_to_token_budget(truncated_item, adjusted_budget)
                    };
                    let Some(adjusted) = adjusted else {
                        continue;
                    };
                    if usize::try_from(estimate_item_token_count_with_audio(
                        &adjusted.item,
                        audio_durations,
                    ))
                    .unwrap_or(usize::MAX)
                        > available_tokens
                    {
                        continue;
                    }
                    truncated_item = adjusted;
                }
            }
            remaining = 0;
            truncated_item
        } else if requires_media_boundary_truncation {
            break;
        } else {
            continue;
        };
        group.keep(&mut selected, source);
    }
    selected
}

fn retained_message_token_count(
    item: &ResponseItem,
    image_budget: RetainedImageBudget,
    audio_durations: &FileAudioDurations,
) -> usize {
    let ResponseItem::Message { content, .. } = item else {
        return usize::try_from(estimate_item_token_count_with_audio(item, audio_durations))
            .unwrap_or(usize::MAX);
    };

    content
        .iter()
        .map(|part| media::content_item_token_count(part, image_budget, audio_durations))
        .fold(0, usize::saturating_add)
}

fn truncate_message_text_to_token_budget(
    mut envelope: ResponseItemEnvelope,
    max_tokens: usize,
) -> Option<ResponseItemEnvelope> {
    let content = to_annotated_content(&mut envelope.item)?;

    let mut remaining = max_tokens;
    let mut truncated_content = Vec::with_capacity(content.len());
    for mut content_item in content {
        match content_item.content_mut() {
            ContentItem::InputText { text } | ContentItem::OutputText { text, .. } => {
                if remaining == 0 {
                    continue;
                }

                let token_count = approx_token_count(text);
                if token_count <= remaining {
                    remaining = remaining.saturating_sub(token_count);
                } else {
                    *text = truncate_text(text, TruncationPolicy::Tokens(remaining));
                    remaining = 0;
                }
                if !text.is_empty() {
                    truncated_content.push(content_item);
                }
            }
            ContentItem::InputImage { .. } | ContentItem::InputAudio { .. } => {
                truncated_content.push(content_item);
            }
        }
    }

    if truncated_content.is_empty() {
        return None;
    }

    set_annotated_content(&mut envelope.item, truncated_content)?;
    if let Some(metadata) = &mut envelope.metadata {
        metadata.mark_retained_sources_incomplete();
    }
    Some(envelope)
}
