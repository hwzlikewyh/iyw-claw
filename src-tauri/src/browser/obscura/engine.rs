use obscura_cdp::dispatch::{dispatch, CdpContext};
use obscura_cdp::types::CdpRequest;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::super::agent_tool_cancellation::cancelled_error;
use super::super::{BrowserError, BrowserErrorCode};
use super::worker::{timeout, unavailable, Request};

pub(super) const SUPPORTED: bool = true;
const SETTLE_MS: u64 = 200;

pub(super) fn run(receiver: mpsc::Receiver<Request>, stop: CancellationToken) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(error = %error, "Could not create the embedded Obscura runtime");
            return;
        }
    };
    // V8 的创建、调用和销毁始终留在同一个操作系统线程。
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tokio::task::LocalSet::new().block_on(&runtime, serve(receiver, stop));
    }));
    if result.is_err() {
        tracing::error!("Embedded Obscura worker panicked and was retired");
    }
}

async fn serve(mut receiver: mpsc::Receiver<Request>, stop: CancellationToken) {
    let mut context = CdpContext::new();
    loop {
        let request = tokio::select! {
            biased;
            _ = stop.cancelled() => break,
            request = receiver.recv() => match request { Some(request) => request, None => break },
        };
        if request.reply.is_closed() {
            continue;
        }
        let result = execute(&mut context, &request, &stop).await;
        retire_abandoned_target(&mut context, &request, &result);
        // 宿主不订阅 CDP 流；每次消费后清空，避免累积页面 payload。
        context.pending_events.clear();
        let _ = request.reply.send(result);
    }
}

async fn execute(
    context: &mut CdpContext,
    request: &Request,
    stop: &CancellationToken,
) -> Result<Value, BrowserError> {
    settle(context, request, stop).await?;
    let remaining = request
        .deadline
        .saturating_duration_since(std::time::Instant::now());
    if remaining.is_zero() {
        return Err(timeout());
    }
    if let Some(page) = context.get_session_page_mut(&request.rpc.session) {
        page.set_navigation_timeout(remaining);
    }
    let command = CdpRequest {
        id: 1,
        method: request.rpc.method.into(),
        params: request.rpc.params.clone(),
        session_id: request.rpc.session.clone(),
    };
    let response = tokio::select! {
        biased;
        _ = stop.cancelled() => return Err(cancelled_error()),
        _ = request.cancellation.cancelled() => return Err(cancelled_error()),
        response = tokio::time::timeout(remaining, dispatch(&command, context)) => response.map_err(|_| timeout())?,
    };
    if let Some(error) = response.error {
        tracing::warn!(
            method = request.rpc.method,
            protocol_code = error.code,
            "Embedded Obscura protocol call failed"
        );
        return Err(unavailable(
            "The embedded Obscura engine could not execute this operation",
        ));
    }
    let value = response.result.unwrap_or(Value::Null);
    if let Some(details) = value.get("exceptionDetails") {
        return Err(evaluation_error(details));
    }
    settle(context, request, stop).await?;
    Ok(value)
}

fn evaluation_error(details: &Value) -> BrowserError {
    let description = details["exception"]["description"]
        .as_str()
        .unwrap_or_default();
    if [
        "STALE_REF",
        "Expected one matching",
        "SyntaxError",
        "selector",
        "not editable",
        "not visible",
        "covered",
        "disabled",
    ]
    .iter()
    .any(|text| description.contains(text))
    {
        return BrowserError::new(
            BrowserErrorCode::BrowserSnapshotStale,
            "Obscura could not resolve the page target; take a fresh snapshot before retrying",
        );
    }
    unavailable("The embedded Obscura engine could not evaluate this page")
}

async fn settle(
    context: &mut CdpContext,
    request: &Request,
    stop: &CancellationToken,
) -> Result<(), BrowserError> {
    if !matches!(
        request.rpc.method,
        "Runtime.evaluate" | "Page.captureScreenshot"
    ) {
        return Ok(());
    }
    let remaining = request
        .deadline
        .saturating_duration_since(std::time::Instant::now());
    let Some(page) = context.get_session_page_mut(&request.rpc.session) else {
        return Ok(());
    };
    tokio::select! {
        biased;
        _ = stop.cancelled() => Err(cancelled_error()),
        _ = request.cancellation.cancelled() => Err(cancelled_error()),
        result = tokio::time::timeout(remaining, page.settle(SETTLE_MS.min(remaining.as_millis() as u64))) => result.map_err(|_| timeout()),
    }
}

fn retire_abandoned_target(
    context: &mut CdpContext,
    request: &Request,
    result: &Result<Value, BrowserError>,
) {
    let target = match (request.rpc.method, result) {
        ("Target.createTarget", Ok(value)) if request.reply.is_closed() => {
            value["targetId"].as_str()
        }
        ("Target.attachToTarget", Err(_)) => request.rpc.params["targetId"].as_str(),
        _ => None,
    };
    if let Some(target) = target {
        context.remove_page(target);
    }
}
