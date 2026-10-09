use super::*;

pub(super) async fn handle_configure(
    state: &HelperState,
    op: HelperOp,
    _stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::Configure {
        driver_path,
        driver_version,
    } = op
    else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    if driver_version != driver::DRIVER_VERSION {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            format!(
                "this helper runs cua-driver {}, not {driver_version}",
                driver::DRIVER_VERSION
            ),
        ));
    }
    let path = PathBuf::from(driver_path);
    let mut current = state.driver_path.lock().await;
    if current.as_ref() != Some(&path) {
        *current = Some(path);
        drop(current);
        // A driver started from another path is not the one iyw-claw now
        // names; stop it, and the next call starts the right one.
        state.shutdown().await;
    }
    value(())
}

pub(super) async fn handle_permissions(
    state: &HelperState,
    op: HelperOp,
    _stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::Permissions = op else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };
    value(state.permissions(true).await)
}

pub(super) async fn handle_list_apps(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::ListApps = op else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    let driver = state.driver(stop).await?;
    value(state.list_apps(&driver).await?)
}

pub(super) async fn handle_find_app(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::FindApp { name, key } = op else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    let driver = state.driver(stop).await?;
    value(ops::find_app(&driver, name.as_deref(), key.as_deref()).await?)
}

pub(super) async fn handle_launch_app(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::LaunchApp { app } = op else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    let driver = state.driver(stop).await?;
    value(ops::launch_app(&driver, &app).await?)
}

pub(super) async fn handle_list_windows(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::ListWindows { pid } = op else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    let driver = state.driver(stop).await?;
    value(state.list_windows(&driver, pid).await?)
}

pub(super) async fn handle_process_start(
    state: &HelperState,
    op: HelperOp,
    _stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::ProcessStart { pid } = op else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };
    value(crate::computer::procinfo::process_start(pid))
}

pub(super) async fn handle_driver_ready(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::DriverReady = op else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    state.driver(stop).await?;
    value(())
}

pub(super) async fn handle_clipboard_read(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::ClipboardRead { expect } = op else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    let driver = state.driver(stop).await?;
    value(ops::clipboard_read(&driver, expect).await?)
}

pub(super) async fn handle_clipboard_write(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::ClipboardWrite { text } = op else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    let driver = state.driver(stop).await?;
    value(ops::clipboard_write(&driver, &text).await?)
}

pub(super) async fn handle_halt(
    state: &HelperState,
    op: HelperOp,
    _stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::Halt { stop } = op else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };

    state.halt(stop).await;
    value(())
}
