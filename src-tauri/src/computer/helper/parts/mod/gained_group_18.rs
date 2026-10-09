// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether `now` grants something `before` did not.
pub(super) fn gained(before: &PermissionReport, now: &PermissionReport) -> bool {
    (now.accessibility && !before.accessibility)
        || (now.screen_recording && !before.screen_recording)
}

/// Serve one op, which iyw-claw let through at Stop count `stop`. A driver call
/// that turns out to lack a permission the remembered answer says is granted
/// clears that answer, so the next call asks the system again rather than
/// going on believing it. (A refusal that came from the remembered answer
/// itself leaves it be: it is asked again on its own schedule.)
pub(super) async fn handle(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let result = handle_op(state, op, stop).await;
    if let Err(HelperError {
        code: HelperErrorCode::PermissionMissing,
        permission: Some(permission),
        ..
    }) = &result
    {
        let mut last = state.permissions.lock().await;
        if last
            .as_ref()
            .is_some_and(|(report, _)| report.has(*permission))
        {
            last.take();
        }
    }
    result
}
