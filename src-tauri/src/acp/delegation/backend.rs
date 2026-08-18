//! Broker transport backend shared by the stdio companion and in-process callers.

use std::io;
use std::sync::Arc;

use crate::acp::delegation::listener::DelegationListener;
use crate::acp::delegation::mutation_gate::MutationLease;

use super::{
    client_cancel, message_round_trip, read_frame, write_frame, BrokerCancelRequest, BrokerMessage,
    BrokerResponse,
};

/// Enough buffering to keep small calls allocation-light while larger frames
/// stream with backpressure through the existing frame codec.
const IN_PROCESS_BUFFER_BYTES: usize = 64 * 1024;

/// One-shot broker transport. Socket mode preserves the companion's existing
/// UDS/named-pipe behavior; in-process mode drives the same listener protocol
/// over an in-memory duplex stream.
#[derive(Clone)]
pub enum DelegationBackend {
    Socket { socket_path: Arc<str> },
    InProcess { listener: Arc<DelegationListener> },
}

impl DelegationBackend {
    pub fn socket(socket_path: impl Into<Arc<str>>) -> Self {
        Self::Socket {
            socket_path: socket_path.into(),
        }
    }

    pub fn in_process(listener: Arc<DelegationListener>) -> Self {
        Self::InProcess { listener }
    }

    pub fn is_in_process(&self) -> bool {
        matches!(self, Self::InProcess { .. })
    }

    pub async fn round_trip(&self, message: BrokerMessage) -> io::Result<BrokerResponse> {
        match self {
            Self::Socket { socket_path } => message_round_trip(socket_path, &message).await,
            Self::InProcess { listener } => {
                in_process_round_trip(Arc::clone(listener), message).await
            }
        }
    }

    pub async fn memory_round_trip(
        &self,
        message: BrokerMessage,
        operation: &'static str,
    ) -> io::Result<BrokerResponse> {
        match self.round_trip(message.clone()).await {
            Ok(response) => Ok(response),
            Err(first_error) => {
                tracing::warn!(
                    target: "user_memory",
                    route = "companion_bridge",
                    operation,
                    error = %first_error,
                    "memory broker transport failed; retrying identical request once"
                );
                self.round_trip(message).await
            }
        }
    }

    pub async fn cancel(&self, request: &BrokerCancelRequest) -> io::Result<()> {
        match self {
            Self::Socket { socket_path } => client_cancel(socket_path, request).await,
            Self::InProcess { .. } => {
                let _ = self
                    .round_trip(BrokerMessage::Cancel(request.clone()))
                    .await?;
                Ok(())
            }
        }
    }

    /// Acquire the host mutation lease for an in-process call. Socket
    /// companions already execute host mutations through the listener, so
    /// their listener-side gate remains the authority.
    pub async fn acquire_mutation(&self, token: &str) -> Option<MutationLease> {
        let Self::InProcess { listener } = self else {
            return None;
        };
        let entry = listener.tokens.lookup(token).await?;
        entry.mutation_gate.acquire(&entry.cancellation).await
    }
}

async fn in_process_round_trip(
    listener: Arc<DelegationListener>,
    message: BrokerMessage,
) -> io::Result<BrokerResponse> {
    let (mut client, mut server) = tokio::io::duplex(IN_PROCESS_BUFFER_BYTES);
    let listener_task = tokio::spawn(async move { listener.serve_one(&mut server).await });
    let response = async {
        write_frame(&mut client, &message).await?;
        read_frame(&mut client).await
    }
    .await;
    drop(client);
    // Dropping this round-trip future detaches the Tokio task instead of
    // aborting it. The client half is dropped at the same time, so operations
    // waiting on input observe EOF, while an already-dispatched mutation can
    // finish its listener-side cleanup before the task exits.
    let listener_result = listener_task.await.map_err(|error| {
        io::Error::other(format!("in-process delegation listener failed: {error}"))
    })?;
    if let Err(error) = listener_result {
        if response.is_ok() {
            return Err(error);
        }
        tracing::warn!(
            target: "delegation",
            error = %error,
            "in-process delegation listener cleanup failed after exchange error"
        );
    }
    response
}
