//! Bounded inbound dedupe + trace-id helpers.
//!
//! Every inbound message carries a `provider_message_id` (the platform's own
//! id when available, else a deterministic composite). The dispatcher keeps a
//! bounded set of recently-seen `(channel_id, provider_message_id)` pairs so
//! duplicate deliveries (poll overlap, WS redelivery, client retries) are
//! dropped instead of spawning duplicate agent turns.

use std::collections::{HashSet, VecDeque};

pub struct InboundDedupe {
    capacity: usize,
    order: VecDeque<(i32, String)>,
    set: HashSet<(i32, String)>,
}

impl InboundDedupe {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            order: VecDeque::new(),
            set: HashSet::new(),
        }
    }

    /// Returns `true` when the message is new (or has no provider id to key
    /// on), `false` when it is a duplicate that must be dropped.
    pub fn check_and_insert(&mut self, channel_id: i32, provider_message_id: &str) -> bool {
        if provider_message_id.trim().is_empty() {
            return true;
        }
        let key = (channel_id, provider_message_id.to_string());
        if !self.set.insert(key.clone()) {
            return false;
        }
        self.order.push_back(key);
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.set.remove(&old);
            }
        }
        true
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

/// Generate an end-to-end trace id for one inbound message. Backends call
/// this once per message; the same id is stamped on every downstream log row.
pub fn new_message_trace_id(channel_id: i32) -> String {
    format!("msg-{}-{}", channel_id, uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_drops_repeats_and_evicts_oldest() {
        let mut dedupe = InboundDedupe::new(3);
        assert!(dedupe.check_and_insert(1, "a"));
        assert!(dedupe.check_and_insert(1, "b"));
        assert!(dedupe.check_and_insert(2, "a"));
        // Duplicate: same channel + same provider id.
        assert!(!dedupe.check_and_insert(1, "a"));
        assert!(dedupe.len() == 3);

        // Eviction: oldest (1,a) falls out after inserting a 4th unique key.
        assert!(dedupe.check_and_insert(3, "c"));
        assert!(dedupe.check_and_insert(1, "a"));
    }

    #[test]
    fn empty_provider_id_is_never_blocked() {
        let mut dedupe = InboundDedupe::new(4);
        assert!(dedupe.check_and_insert(1, ""));
        assert!(dedupe.check_and_insert(1, ""));
        assert!(dedupe.len() == 0);
    }

    #[test]
    fn trace_id_is_unique_and_scoped() {
        let a = new_message_trace_id(7);
        let b = new_message_trace_id(7);
        assert_ne!(a, b);
        assert!(a.starts_with("msg-7-"));
    }
}
