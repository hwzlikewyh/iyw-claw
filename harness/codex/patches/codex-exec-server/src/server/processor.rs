use std::sync::Arc;
use std::time::Instant;

use codex_exec_server_protocol::JSONRPCMessage;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::warn;

use crate::ExecServerRuntimePaths;
use crate::connection::CHANNEL_CAPACITY;
use crate::connection::JsonRpcConnection;
use crate::connection::JsonRpcConnectionEvent;
use crate::rpc::RpcCallError;
use crate::rpc::RpcNotificationSender;
use crate::rpc::RpcServerOutboundMessage;
use crate::rpc::encode_server_message;
use crate::rpc_server_requests::RpcServerRequestSender;
use crate::server::ExecServerHandler;
use crate::server::RequestDispatchMode;
use crate::server::registry::build_router;
use crate::server::request_dispatcher::RequestDispatcher;
use crate::server::request_dispatcher::RequestTaskResult;
use crate::server::session_registry::SessionRegistry;
use crate::telemetry::ConnectionTransport;
use crate::telemetry::ExecServerTelemetry;
use codex_http_client::HttpClientFactory;

#[derive(Clone)]
pub(crate) struct ConnectionProcessor {
    session_registry: Arc<SessionRegistry>,
    runtime_paths: ExecServerRuntimePaths,
    telemetry: ExecServerTelemetry,
    http_client_factory: HttpClientFactory,
    request_dispatch_mode: RequestDispatchMode,
}

impl ConnectionProcessor {

    pub(crate) fn new_with_telemetry(
        runtime_paths: ExecServerRuntimePaths,
        telemetry: ExecServerTelemetry,
        http_client_factory: HttpClientFactory,
        request_dispatch_mode: RequestDispatchMode,
    ) -> Self {
        Self {
            session_registry: SessionRegistry::new(telemetry.clone()),
            runtime_paths,
            telemetry,
            http_client_factory,
            request_dispatch_mode,
        }
    }

    pub(crate) async fn run_connection(
        &self,
        connection: JsonRpcConnection,
        transport: ConnectionTransport,
    ) {
        run_connection(
            connection,
            Arc::clone(&self.session_registry),
            self.runtime_paths.clone(),
            self.telemetry.clone(),
            self.http_client_factory.clone(),
            transport,
            self.request_dispatch_mode,
        )
        .await;
    }

    pub(crate) async fn shutdown(&self) {
        self.session_registry.shutdown().await;
    }
}

async fn run_connection(
    connection: JsonRpcConnection,
    session_registry: Arc<SessionRegistry>,
    runtime_paths: ExecServerRuntimePaths,
    telemetry: ExecServerTelemetry,
    http_client_factory: HttpClientFactory,
    transport: ConnectionTransport,
    request_dispatch_mode: RequestDispatchMode,
) {
    let _connection_metrics = telemetry.connection_started(transport);
    let JsonRpcConnection {
        outgoing_tx: json_outgoing_tx,
        mut incoming_rx,
        mut disconnected_rx,
        task_handles: connection_tasks,
        transport: _transport,
    } = connection;
    let (outgoing_tx, mut outgoing_rx) =
        mpsc::channel::<RpcServerOutboundMessage>(CHANNEL_CAPACITY);
    let notifications = RpcNotificationSender::new(outgoing_tx.clone());
    let requests = notifications.request_sender();
    let handler = Arc::new(ExecServerHandler::new(
        session_registry,
        notifications,
        runtime_paths,
        http_client_factory,
    ));

    let outbound_task = tokio::spawn(async move {
        while let Some(message) = outgoing_rx.recv().await {
            let json_message = match encode_server_message(message) {
                Ok(json_message) => json_message,
                Err(err) => {
                    warn!("failed to serialize exec-server outbound message: {err}");
                    break;
                }
            };
            if json_outgoing_tx.send(json_message).await.is_err() {
                break;
            }
        }
    });

    let mut dispatcher = RequestDispatcher::new(
        Arc::new(build_router()),
        Arc::clone(&handler),
        outgoing_tx.clone(),
        disconnected_rx.clone(),
        requests.clone(),
        telemetry,
        request_dispatch_mode,
    );

    loop {
        let has_request_tasks = dispatcher.has_tasks();
        let event = tokio::select! {
            result = dispatcher.join_next(), if has_request_tasks => {
                if result == RequestTaskResult::ConnectionClosed {
                    break;
                }
                continue;
            }
            _ = disconnected_rx.changed() => {
                debug!("exec-server transport disconnected");
                break;
            }
            event = incoming_rx.recv() => {
                let Some(event) = event else {
                    break;
                };
                event
            }
        };

        if !handler.is_session_attached() {
            debug!("exec-server connection evicted after session resume");
            break;
        }

        let result = match event {
            JsonRpcConnectionEvent::MalformedMessage { reason } => {
                dispatcher.handle_malformed_message(reason).await
            }
            JsonRpcConnectionEvent::Message(message) => match message {
                JSONRPCMessage::Request(request) => {
                    dispatcher
                        .dispatch_request(request, tracing::Span::none(), Instant::now())
                        .await
                }
                JSONRPCMessage::Notification(notification) => {
                    dispatcher.handle_notification(notification).await
                }
                JSONRPCMessage::Response(response) => dispatcher.handle_response(response),
                JSONRPCMessage::Error(error) => dispatcher.handle_error(error),
            },
            JsonRpcConnectionEvent::QueuedRequest {
                request,
                request_span,
                queued_at,
            } => {
                dispatcher
                    .dispatch_request(request, request_span, queued_at)
                    .await
            }
            JsonRpcConnectionEvent::Disconnected { reason } => {
                if let Some(reason) = reason {
                    debug!("exec-server connection disconnected: {reason}");
                }
                break;
            }
        };
        if result == RequestTaskResult::ConnectionClosed {
            break;
        }
    }

    if *disconnected_rx.borrow() {
        complete_queued_client_responses(&requests, &mut incoming_rx);
    }
    requests.close();
    dispatcher.shutdown().await;
    handler.shutdown().await;
    drop(handler);
    drop(requests);
    drop(outgoing_tx);
    for task in connection_tasks {
        task.abort();
        let _ = task.await;
    }
    let _ = outbound_task.await;
}

fn complete_queued_client_responses(
    requests: &RpcServerRequestSender,
    incoming_rx: &mut mpsc::Receiver<JsonRpcConnectionEvent>,
) {
    while let Ok(event) = incoming_rx.try_recv() {
        let (request_id, response) = match event {
            JsonRpcConnectionEvent::Message(JSONRPCMessage::Response(response)) => {
                (response.id, Ok(response.result))
            }
            JsonRpcConnectionEvent::Message(JSONRPCMessage::Error(error)) => {
                (error.id, Err(RpcCallError::Server(error.error)))
            }
            JsonRpcConnectionEvent::Message(
                JSONRPCMessage::Request(_) | JSONRPCMessage::Notification(_),
            )
            | JsonRpcConnectionEvent::QueuedRequest { .. }
            | JsonRpcConnectionEvent::MalformedMessage { .. }
            | JsonRpcConnectionEvent::Disconnected { .. } => continue,
        };
        if !requests.complete(request_id.clone(), response) {
            warn!("ignoring unexpected client response while disconnecting: {request_id:?}");
        }
    }
}
