use super::*;

pub(super) fn value(v: impl serde::Serialize) -> Result<serde_json::Value, HelperError> {
    serde_json::to_value(v).map_err(|e| HelperError::failed(format!("encode: {e}")))
}

/// 只调度完整的协议操作，权限与停止校验由各操作在实际执行前完成。
pub(super) async fn handle_op(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    match op {
        op @ HelperOp::Configure { .. } => handle_configure(state, op, stop).await,
        op @ HelperOp::Permissions => handle_permissions(state, op, stop).await,
        op @ HelperOp::ListApps => handle_list_apps(state, op, stop).await,
        op @ HelperOp::FindApp { .. } => handle_find_app(state, op, stop).await,
        op @ HelperOp::LaunchApp { .. } => handle_launch_app(state, op, stop).await,
        op @ HelperOp::ListWindows { .. } => handle_list_windows(state, op, stop).await,
        op @ HelperOp::ProcessStart { .. } => handle_process_start(state, op, stop).await,
        op @ HelperOp::Capture { .. } => handle_capture(state, op, stop).await,
        op @ HelperOp::Snapshot { .. } => handle_snapshot(state, op, stop).await,
        op @ HelperOp::Verify { .. } => handle_verify(state, op, stop).await,
        op @ HelperOp::Act { .. } => handle_act(state, op, stop).await,
        op @ HelperOp::CaptureScreen { .. } => handle_capture_screen(state, op, stop).await,
        op @ HelperOp::DriverReady => handle_driver_ready(state, op, stop).await,
        op @ HelperOp::ActScreen { .. } => handle_act_screen(state, op, stop).await,
        op @ HelperOp::ClipboardRead { .. } => handle_clipboard_read(state, op, stop).await,
        op @ HelperOp::ClipboardWrite { .. } => handle_clipboard_write(state, op, stop).await,
        op @ HelperOp::Halt { .. } => handle_halt(state, op, stop).await,
    }
}
