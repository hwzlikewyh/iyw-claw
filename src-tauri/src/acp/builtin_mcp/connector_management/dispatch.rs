use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::super::authority::SessionContext;
use super::super::delivery::RelayDelivery;
use super::super::handler::BuiltinMcpHandler;
use super::request::Request;
use super::{market, publish};

pub(super) async fn execute(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request: Request,
    request_id: Value,
    request_cancel: CancellationToken,
) -> Result<CallToolResult, ErrorData> {
    if request.is_read() {
        return read(handler, request).await;
    }
    if request.is_publish() {
        return publish_request(
            handler,
            authority,
            delivery,
            request,
            request_id,
            request_cancel,
        )
        .await;
    }
    manage(
        handler,
        authority,
        delivery,
        request,
        request_id,
        request_cancel,
    )
    .await
}

async fn read(handler: &BuiltinMcpHandler, request: Request) -> Result<CallToolResult, ErrorData> {
    match request {
        Request::InstalledList => market::installed_list(handler.gateway_service()).await,
        Request::MarketSearch {
            query,
            provider_id,
            limit,
            offset,
        } => market::search(handler.gateway_service(), query, provider_id, limit, offset).await,
        Request::MarketDetail {
            provider_id,
            server_id,
        } => market::detail(handler.gateway_service(), provider_id, server_id).await,
        _ => Err(unknown_request()),
    }
}

async fn manage(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request: Request,
    request_id: Value,
    request_cancel: CancellationToken,
) -> Result<CallToolResult, ErrorData> {
    match request {
        Request::InstallMarket {
            provider_id,
            server_id,
            option_id,
            protocol,
            parameter_values,
        } => {
            market::install(
                handler,
                authority,
                delivery,
                request_id,
                request_cancel,
                (
                    provider_id,
                    server_id,
                    option_id,
                    protocol,
                    parameter_values,
                ),
            )
            .await
        }
        Request::Uninstall { server_id } => {
            market::uninstall(
                handler,
                authority,
                delivery,
                request_id,
                request_cancel,
                server_id,
            )
            .await
        }
        Request::SetCurrentConversation { server_id, enabled } => {
            super::set_current_conversation(
                handler,
                authority,
                delivery,
                request_id,
                request_cancel,
                server_id,
                enabled,
            )
            .await
        }
        Request::SetGlobalEnabled { server_id, enabled } => {
            market::set_global_enabled(
                handler,
                authority,
                delivery,
                request_id,
                request_cancel,
                server_id,
                enabled,
            )
            .await
        }
        _ => Err(unknown_request()),
    }
}

async fn publish_request(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request: Request,
    request_id: Value,
    request_cancel: CancellationToken,
) -> Result<CallToolResult, ErrorData> {
    match request {
        Request::PublishPlugin { request } => {
            publish::plugin(
                handler,
                authority,
                delivery,
                request_id,
                request_cancel,
                request,
            )
            .await
        }
        Request::PublishPluginVersion { request } => {
            publish::version(
                handler,
                authority,
                delivery,
                request_id,
                request_cancel,
                request,
            )
            .await
        }
        Request::UpdateMarketMetadata { request } => {
            publish::update_metadata(
                handler,
                authority,
                delivery,
                request_id,
                request_cancel,
                request,
            )
            .await
        }
        Request::DeleteMarketItem { market_id } => {
            publish::delete(
                handler,
                authority,
                delivery,
                request_id,
                request_cancel,
                market_id,
            )
            .await
        }
        _ => Err(unknown_request()),
    }
}

fn unknown_request() -> ErrorData {
    ErrorData::invalid_params("connector operation is invalid", None)
}
