//! Preserve the instruction provider referenced by upstream library constructors.

use std::sync::Arc;

use codex_extension_api::LoadInstructionsFuture;
use codex_extension_api::LoadedUserInstructions;
use codex_extension_api::UserInstructionsProvider;

/// Retains the active turn's timing after completion without exporting telemetry.
/// Returns `None` when there is no running task. The observer reports exclusive
/// durations for the caller's interval in inference, automatic review, local
/// tools, overhead, and unknown order.
pub async fn observe_turn_timing(
    thread: &crate::CodexThread,
) -> Option<
    impl Fn(std::time::Instant, std::time::Instant) -> Option<[std::time::Duration; 5]> + use<>,
> {
    let active = thread.session.active_turn.lock().await;
    let timing = Arc::clone(
        &active
            .as_ref()?
            .task
            .as_ref()?
            .turn_context
            .turn_timing_state,
    );
    Some(move |start, end| {
        timing.snapshot_path(end).map(|path| {
            let totals = path.durations(start, end);
            [
                totals.inference,
                totals.auto_review,
                totals.tools,
                totals.overhead,
                totals.unknown,
            ]
        })
    })
}

/// Test-only provider that supplies no user instructions.
#[derive(Debug, Default)]
pub struct EmptyUserInstructionsProvider;

impl UserInstructionsProvider for EmptyUserInstructionsProvider {
    fn load_user_instructions(&self) -> LoadInstructionsFuture<'_> {
        Box::pin(async { LoadedUserInstructions::default() })
    }
}
