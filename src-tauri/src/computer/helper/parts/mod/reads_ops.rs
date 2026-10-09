use super::*;

pub(super) async fn handle_capture(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::Capture {
        pid,
        window_id,
        max_dimension,
    } = op
    else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    state.require(OsPermission::ScreenRecording).await?;
    #[cfg(target_os = "macos")]
    if let Some(why) = state.out_of_sight(pid, window_id).await {
        return Err(ops::out_of_sight_capture(why));
    }
    let driver = state.driver(stop).await?;
    let captured = ops::capture(&driver, pid, window_id, max_dimension).await;
    // Where a capture replaces the window's snapshot, the refs from
    // the one before name nothing any more — whether or not this
    // capture is handed on.
    if ops::capture_replaces_snapshot() {
        state.snapshots().record(pid, window_id, None);
    }
    value(captured?)
}

pub(super) async fn handle_snapshot(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::Snapshot {
        pid,
        window_id,
        max_depth,
        max_elements,
        query,
        app_menus,
    } = op
    else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    state.require(OsPermission::Accessibility).await?;
    let driver = state.driver(stop).await?;
    let (raw, facts) = ops::snapshot(
        &driver,
        pid,
        window_id,
        max_depth,
        max_elements,
        query,
        app_menus,
    )
    .await?;
    state.snapshots().record(pid, window_id, facts);
    value(raw)
}

pub(super) async fn handle_verify(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::Verify {
        pid,
        window_id,
        request,
    } = op
    else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    if request.expect.iter().any(|p| p.element.is_some()) {
        state.require(OsPermission::Accessibility).await?;
    }
    let driver = state.driver(stop).await?;
    value(ops::verify(&driver, pid, window_id, &request).await?)
}

pub(super) async fn handle_capture_screen(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::CaptureScreen {
        rules,
        max_dimension,
    } = op
    else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    state.require(OsPermission::ScreenRecording).await?;
    screen_offered()?;
    let driver = state.driver(stop).await?;
    value(screen::capture(&driver, &rules, max_dimension).await?)
}
