use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use rmcp::model::{
    ClientJsonRpcMessage, GetExtensions, JsonRpcMessage, RequestId, ServerJsonRpcMessage,
};
use rmcp::transport::{async_rw::JsonRpcMessageCodec, Transport};
use rmcp::RoleServer;
use tokio::io::{DuplexStream, ReadHalf, WriteHalf};
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio_util::sync::CancellationToken;

use super::super::{authority::SessionContext, delivery::RelayDelivery};

const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_PENDING_REQUESTS: usize = 256;
type Writer = FramedWrite<WriteHalf<DuplexStream>, JsonRpcMessageCodec<ServerJsonRpcMessage>>;

#[derive(Clone)]
pub(in crate::acp::builtin_mcp) struct RequestContext {
    pub authority: SessionContext,
    pub delivery: RelayDelivery,
}

#[derive(Default)]
struct Deliveries(Mutex<HashMap<RequestId, RelayDelivery>>);

impl Drop for Deliveries {
    fn drop(&mut self) {
        for delivery in self
            .0
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .values()
        {
            delivery.abort();
        }
    }
}

pub(super) struct InProcessTransport {
    read: FramedRead<ReadHalf<DuplexStream>, JsonRpcMessageCodec<ClientJsonRpcMessage>>,
    write: Arc<tokio::sync::Mutex<Option<Writer>>>,
    deliveries: Arc<Deliveries>,
    authority: SessionContext,
    stop: CancellationToken,
}

impl InProcessTransport {
    pub(super) fn new(
        stream: DuplexStream,
        authority: SessionContext,
        stop: CancellationToken,
    ) -> Self {
        let (read, write) = tokio::io::split(stream);
        Self {
            read: FramedRead::new(
                read,
                JsonRpcMessageCodec::new_with_max_length(MAX_MESSAGE_BYTES),
            ),
            write: Arc::new(tokio::sync::Mutex::new(Some(FramedWrite::new(
                write,
                JsonRpcMessageCodec::new_with_max_length(MAX_MESSAGE_BYTES),
            )))),
            deliveries: Arc::default(),
            authority,
            stop,
        }
    }

    fn prepare(&self, message: &mut ClientJsonRpcMessage) -> io::Result<()> {
        let JsonRpcMessage::Request(request) = message else {
            return Ok(());
        };
        let mut deliveries = self
            .deliveries
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if deliveries.len() >= MAX_PENDING_REQUESTS || deliveries.contains_key(&request.id) {
            return Err(io::Error::other(
                "in-process MCP request capacity or duplicate ID",
            ));
        }
        let delivery = RelayDelivery::default();
        let mut authority = self.authority.clone();
        authority.capture_tools_generation();
        request.request.extensions_mut().insert(RequestContext {
            authority,
            delivery: delivery.clone(),
        });
        deliveries.insert(request.id.clone(), delivery);
        Ok(())
    }
}

impl Transport<RoleServer> for InProcessTransport {
    type Error = io::Error;

    fn send(
        &mut self,
        message: ServerJsonRpcMessage,
    ) -> impl Future<Output = io::Result<()>> + Send + 'static {
        let id = match &message {
            JsonRpcMessage::Response(response) => Some(&response.id),
            JsonRpcMessage::Error(error) => Some(&error.id),
            _ => None,
        };
        let delivery = id.and_then(|id| {
            self.deliveries
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(id)
        });
        let guard = DeliveryGuard(delivery);
        let writer = Arc::clone(&self.write);
        let stop = self.stop.clone();
        let authority = self.authority.cancellation().clone();
        async move {
            tokio::select! {
                biased;
                _ = stop.cancelled() => Err(io::Error::other("MCP transport closed")),
                _ = authority.cancelled() => Err(io::Error::other("MCP authority revoked")),
                result = write_message(writer, message) => {
                    result?;
                    if let Some(delivery) = &guard.0 { delivery.complete(); }
                    Ok(())
                }
            }
        }
    }

    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        let next = tokio::select! {
            biased;
            _ = self.stop.cancelled() => return None,
            _ = self.authority.cancellation().cancelled() => {
                self.stop.cancel();
                return None;
            },
            next = self.read.next() => next,
        };
        let Some(next) = next else {
            self.stop.cancel();
            return None;
        };
        match next.map_err(io::Error::other).and_then(|mut message| {
            self.prepare(&mut message)?;
            Ok(message)
        }) {
            Ok(message) => Some(message),
            Err(_) => {
                self.stop.cancel();
                tracing::warn!(target: "builtin_mcp", connection_id = self.authority.connection_id(),
                    "invalid or oversized in-process MCP frame");
                None
            }
        }
    }

    async fn close(&mut self) -> io::Result<()> {
        self.stop.cancel();
        self.write.lock().await.take();
        let mut deliveries = self
            .deliveries
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for (_, delivery) in deliveries.drain() {
            delivery.abort();
        }
        Ok(())
    }
}

impl Drop for InProcessTransport {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

async fn write_message(
    writer: Arc<tokio::sync::Mutex<Option<Writer>>>,
    message: ServerJsonRpcMessage,
) -> io::Result<()> {
    let mut writer = writer.lock().await;
    writer
        .as_mut()
        .ok_or_else(|| io::Error::other("MCP writer closed"))?
        .send(message)
        .await
        .map_err(io::Error::other)
}

struct DeliveryGuard(Option<RelayDelivery>);

impl Drop for DeliveryGuard {
    fn drop(&mut self) {
        if let Some(delivery) = &self.0 {
            delivery.abort();
        }
    }
}
