use std::collections::{HashMap, VecDeque};

use sacp::schema::{
    RequestPermissionOutcome, RequestPermissionResponse, SelectedPermissionOutcome,
};
use sacp::Responder;

use crate::acp::types::PermissionOptionInfo;

pub(crate) trait PermissionResponder {
    fn respond_selected(self, option_id: String) -> bool;
    fn respond_cancelled(self) -> bool;
}

impl PermissionResponder for Responder<RequestPermissionResponse> {
    fn respond_selected(self, option_id: String) -> bool {
        let outcome = RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id));
        self.respond(RequestPermissionResponse::new(outcome))
            .is_err()
    }

    fn respond_cancelled(self) -> bool {
        self.respond(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ))
        .is_err()
    }
}

pub(crate) struct QueuedPermission {
    pub(crate) request_id: String,
    pub(crate) tool_call: serde_json::Value,
    pub(crate) options: Vec<PermissionOptionInfo>,
}

pub(crate) struct PermissionResolution {
    pub(crate) answered: bool,
    pub(crate) delivery_failed: bool,
    pub(crate) next: Option<QueuedPermission>,
}

pub(crate) struct PermissionDrain {
    pub(crate) visible_request_id: Option<String>,
    pub(crate) count: usize,
    pub(crate) delivery_failures: usize,
}

pub(crate) enum PermissionAdmission {
    Visible(QueuedPermission),
    Queued,
    Closed { delivery_failed: bool },
}

pub(crate) struct PermissionQueue<R> {
    responders: HashMap<String, R>,
    visible_request_id: Option<String>,
    waiting: VecDeque<QueuedPermission>,
    closed: bool,
}

impl<R> Default for PermissionQueue<R> {
    fn default() -> Self {
        Self {
            responders: HashMap::new(),
            visible_request_id: None,
            waiting: VecDeque::new(),
            closed: false,
        }
    }
}

impl<R: PermissionResponder> PermissionQueue<R> {
    pub(crate) fn admit(&mut self, responder: R, card: QueuedPermission) -> PermissionAdmission {
        if self.closed {
            return PermissionAdmission::Closed {
                delivery_failed: responder.respond_cancelled(),
            };
        }
        let request_id = card.request_id.clone();
        debug_assert!(!self.responders.contains_key(&request_id));
        self.responders.insert(request_id.clone(), responder);
        if self.visible_request_id.is_none() {
            self.visible_request_id = Some(request_id);
            PermissionAdmission::Visible(card)
        } else {
            self.waiting.push_back(card);
            PermissionAdmission::Queued
        }
    }

    pub(crate) fn resolve(&mut self, request_id: &str, option_id: String) -> PermissionResolution {
        if self.visible_request_id.as_deref() != Some(request_id) {
            return PermissionResolution {
                answered: false,
                delivery_failed: false,
                next: None,
            };
        }
        let Some(responder) = self.responders.remove(request_id) else {
            return PermissionResolution {
                answered: false,
                delivery_failed: false,
                next: None,
            };
        };
        let delivery_failed = responder.respond_selected(option_id);
        let next = self.waiting.pop_front();
        self.visible_request_id = next.as_ref().map(|card| card.request_id.clone());
        PermissionResolution {
            answered: true,
            delivery_failed,
            next,
        }
    }

    pub(crate) fn drain(&mut self) -> PermissionDrain {
        let count = self.responders.len();
        let delivery_failures = self
            .responders
            .drain()
            .map(|(_, responder)| usize::from(responder.respond_cancelled()))
            .sum();
        self.waiting.clear();
        PermissionDrain {
            visible_request_id: self.visible_request_id.take(),
            count,
            delivery_failures,
        }
    }

    pub(crate) fn close_and_drain(&mut self) -> PermissionDrain {
        self.closed = true;
        self.drain()
    }

    pub(crate) fn waiting_len(&self) -> usize {
        self.waiting.len()
    }

    pub(crate) fn visible_request_id(&self) -> Option<&str> {
        self.visible_request_id.as_deref()
    }

    pub(crate) fn len(&self) -> usize {
        self.responders.len()
    }
}
