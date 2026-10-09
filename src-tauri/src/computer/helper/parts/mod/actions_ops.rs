use super::*;
use crate::computer::protocol::{ClipboardUse, WindowAction};
use crate::computer::types::{ActDelivery, Rect};

struct WindowOperation {
    pid: u32,
    window_id: u64,
    started_at: u64,
    content: Option<ProcessRun>,
    app_key: Option<String>,
    action: WindowAction,
    mode: ActDelivery,
    clipboard: ClipboardUse,
}

impl WindowOperation {
    fn parse(op: HelperOp) -> Result<Self, HelperError> {
        let HelperOp::Act {
            pid,
            window_id,
            started_at,
            content,
            app_key,
            action,
            delivery: mode,
            clipboard,
        } = op
        else {
            return Err(HelperError::new(
                HelperErrorCode::BadRequest,
                "Operation dispatch does not match its request",
            ));
        };
        Ok(Self {
            pid,
            window_id,
            started_at,
            content,
            app_key,
            action,
            mode,
            clipboard,
        })
    }
}

pub(super) async fn handle_act(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let request = WindowOperation::parse(op)?;
    let delivery = state.delivery(
        request.pid,
        request.window_id,
        request.started_at,
        request.content,
        stop,
    );
    // 先核对停止与进程身份，拒绝的动作不启动驱动。
    delivery.check()?;
    for permission in act::permissions_for(&request.action) {
        state.require(*permission).await?;
    }
    let driver = state.driver(stop).await?;
    let paste_ok = may_paste(&driver, &request.clipboard).await?;
    let call = act::ActionCall {
        driver: &driver,
        pid: request.pid,
        window_id: request.window_id,
        mode: request.mode,
        deliverable: &delivery,
        paste_ok,
    };
    value(run_window_action(state, &call, &request).await?)
}

async fn may_paste(driver: &DriverProc, use_of: &ClipboardUse) -> Result<bool, HelperError> {
    match use_of.paste {
        Some(expect) => {
            let now = clipboard::stamp(driver).await?;
            Ok(now.value == expect && !now.concealed)
        }
        None => Ok(false),
    }
}

async fn run_window_action(
    state: &HelperState,
    call: &act::ActionCall<'_>,
    request: &WindowOperation,
) -> Result<RawAct, HelperError> {
    let action = &request.action;
    if !call.paste_ok && act::pastes(action) {
        return Err(act::paste_refused());
    }
    let element_frame = action_element_frame(state, call, request)?;
    let mut window_frame =
        act::check_points(call.driver, call.pid, call.window_id, &action.points()).await?;
    let before = if request.clipboard.track {
        Some(clipboard::stamp(call.driver).await?)
    } else {
        None
    };
    let done = match act::act(call, action).await {
        Err(error) if act::needs_capture(&error) => {
            // 仅在驱动明确未执行、缺少截图时补充截图；未知执行结果不重放。
            if ops::publish_capture(call.driver, call.pid, call.window_id).await? {
                state.snapshots().record(call.pid, call.window_id, None);
            }
            window_frame =
                act::check_points(call.driver, call.pid, call.window_id, &action.points()).await?;
            act::act(call, action).await?
        }
        done => done?,
    };
    let copied = match before {
        Some(before) => clipboard::changed_since(call.driver, before.value).await,
        None => None,
    };
    Ok(RawAct {
        element_frame,
        window_frame,
        clipboard: copied,
        ..done
    })
}

fn action_element_frame(
    state: &HelperState,
    call: &act::ActionCall<'_>,
    request: &WindowOperation,
) -> Result<Option<Rect>, HelperError> {
    let book = state.snapshots();
    book.check(
        call.pid,
        call.window_id,
        &request.action,
        request.app_key.as_deref(),
        call.paste_ok,
    )?;
    Ok(request
        .action
        .element()
        .and_then(|element| book.frame(call.pid, call.window_id, element)))
}

pub(super) async fn handle_act_screen(
    state: &HelperState,
    op: HelperOp,
    stop: u64,
) -> Result<serde_json::Value, HelperError> {
    let HelperOp::ActScreen {
        rules,
        action,
        geometry,
    } = op
    else {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "Operation dispatch does not match its request",
        ));
    };
    for permission in act::permissions_for(&action) {
        state.require(*permission).await?;
    }
    screen_offered()?;
    let driver = state.running_driver(stop).await?;
    let stopped_now = state.stopped.clone();
    let ready = move || {
        if stopped_now.load(Ordering::Acquire) > stop {
            return Err(stopped());
        }
        unlocked_screen()
    };
    ready()?;
    value(screen::act(&driver, &rules, &action, geometry, ready).await?)
}

fn unlocked_screen() -> Result<(), HelperError> {
    match session::state() {
        session::SessionState::Unlocked => Ok(()),
        session::SessionState::Locked => Err(HelperError::new(HelperErrorCode::Paused, "The screen is locked, or another user's session is active.")),
        session::SessionState::Unknown => Err(HelperError::new(HelperErrorCode::ActionFailed,
            "iyw-claw cannot tell whether this desktop's session is locked, so it does not act on the screen here.")),
    }
}
