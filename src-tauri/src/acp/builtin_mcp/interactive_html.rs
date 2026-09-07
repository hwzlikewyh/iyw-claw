use std::sync::Arc;

use rmcp::{
    model::{CallToolResult, Content},
    ErrorData,
};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::{authority::SessionContext, handler::BuiltinMcpHandler, invocation::ensure_active};
use crate::acp::{interactive_html::InteractiveHtmlRequest, question::SessionQuestionAccess};

impl BuiltinMcpHandler {
    pub(super) async fn show_interactive_html(
        &self,
        authority: SessionContext,
        request: InteractiveHtmlRequest,
        request_cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        ensure_active(&authority, &request_cancel)?;
        let access = self.invocation_dependencies().listener.html_access();
        let stop = request_cancel.child_token();
        // 请求 future 被丢弃时仍由任务完成取消和页面清理。
        let _guard = stop.clone().drop_guard();
        let result = tokio::spawn(run_html(HtmlCall {
            access,
            authority,
            request,
            stop,
        }))
        .await
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))??;
        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }
}

struct HtmlCall {
    access: Arc<dyn SessionQuestionAccess>,
    authority: SessionContext,
    request: InteractiveHtmlRequest,
    stop: CancellationToken,
}

async fn run_html(call: HtmlCall) -> Result<Value, ErrorData> {
    ensure_active(&call.authority, &call.stop)?;
    let conn_id = call.authority.connection_id();
    let registered = call
        .access
        .present_html(conn_id, call.request)
        .await
        .map_err(|error| ErrorData::invalid_request(error, None))?;
    let id = registered.interaction_id;
    if let Err(error) = ensure_active(&call.authority, &call.stop) {
        call.access.cancel_html(conn_id, &id).await;
        return Err(error);
    }
    let Some(receiver) = registered.answer_rx else {
        watch_display_page(
            call.access,
            call.authority,
            (id.clone(), registered.cancellation),
        );
        return Ok(json!({"interaction_id": id, "status": "presented"}));
    };
    let result = tokio::select! {
        biased;
        answer = receiver => answer.ok(),
        _ = registered.cancellation.cancelled() => None,
        _ = call.stop.cancelled() => None,
        _ = call.authority.cancellation().cancelled() => None,
    };
    call.access.cancel_html(conn_id, &id).await;
    Ok(result.unwrap_or_else(|| json!({"interaction_id": id, "status": "cancelled"})))
}

fn watch_display_page(
    access: Arc<dyn SessionQuestionAccess>,
    authority: SessionContext,
    (interaction_id, cancellation): (String, CancellationToken),
) {
    tokio::spawn(async move {
        tokio::select! {
            _ = authority.cancellation().cancelled() => {},
            _ = cancellation.cancelled() => {},
        }
        access
            .cancel_html(authority.connection_id(), &interaction_id)
            .await;
    });
}
