use std::sync::{Arc, Mutex};
use std::task::Poll;

use axum::body::Body;
use axum::response::Response;
use futures_util::stream::poll_fn;
use futures_util::Stream;
use tokio::sync::OwnedSemaphorePermit;

type RelayCallback = Box<dyn FnOnce() + Send + 'static>;

struct PendingDelivery {
    on_complete: RelayCallback,
    on_abort: RelayCallback,
}

enum DeliveryState {
    Open(Vec<PendingDelivery>),
    Completed,
    Aborted,
}

impl Default for DeliveryState {
    fn default() -> Self {
        Self::Open(Vec::new())
    }
}

#[derive(Clone, Default)]
pub(super) struct RelayDelivery {
    state: Arc<Mutex<DeliveryState>>,
}

impl RelayDelivery {
    pub(super) fn register(&self, on_complete: RelayCallback, on_abort: RelayCallback) {
        let callback = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match &mut *state {
                DeliveryState::Open(pending) => {
                    pending.push(PendingDelivery {
                        on_complete,
                        on_abort,
                    });
                    return;
                }
                DeliveryState::Completed => on_complete,
                DeliveryState::Aborted => on_abort,
            }
        };
        callback();
    }

    fn complete(&self) {
        let callbacks = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let callbacks = match &mut *state {
                DeliveryState::Open(pending) => std::mem::take(pending),
                DeliveryState::Completed | DeliveryState::Aborted => return,
            };
            *state = DeliveryState::Completed;
            callbacks
        };
        for PendingDelivery { on_complete, .. } in callbacks {
            on_complete();
        }
    }

    pub(super) fn abort(&self) {
        let callbacks = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let callbacks = match &mut *state {
                DeliveryState::Open(pending) => std::mem::take(pending),
                DeliveryState::Completed | DeliveryState::Aborted => return,
            };
            *state = DeliveryState::Aborted;
            callbacks
        };
        for PendingDelivery { on_abort, .. } in callbacks {
            on_abort();
        }
    }
}

struct RelayGuard(RelayDelivery);

impl Drop for RelayGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) fn wrap_delivery(
    response: &mut Response,
    delivery: RelayDelivery,
    permits: (OwnedSemaphorePermit, OwnedSemaphorePermit),
) {
    let body = std::mem::replace(response.body_mut(), Body::empty());
    let mut stream = Box::pin(body.into_data_stream());
    let mut failed = false;
    let relay = RelayGuard(delivery);
    let stream = poll_fn(move |context| {
        let _ = &permits;
        match stream.as_mut().poll_next(context) {
            Poll::Ready(None) => {
                if !failed {
                    relay.0.complete();
                }
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                failed = true;
                relay.0.abort();
                Poll::Ready(Some(Err(error)))
            }
            other => other,
        }
    });
    *response.body_mut() = Body::from_stream(stream);
}
