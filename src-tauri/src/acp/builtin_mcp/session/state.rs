use std::collections::HashMap;
use std::time::Instant;

use super::{SessionRecord, MAX_REVOKED_PARENTS, REVOKED_PARENT_TTL};
use crate::acp::builtin_mcp::authority::SessionContext;
use crate::acp::builtin_mcp::credential::TokenDigest;

pub(super) fn take_matching(
    sessions: &mut HashMap<TokenDigest, SessionRecord>,
    connection_id: &str,
) -> Vec<SessionContext> {
    let mut removed = Vec::new();
    sessions.retain(|_, record| {
        if record.context.connection_id() == connection_id {
            removed.push(record.context.clone());
            false
        } else {
            true
        }
    });
    removed
}

pub(super) fn cancel_contexts(contexts: Vec<SessionContext>) -> usize {
    let count = contexts.len();
    for context in contexts {
        context.cancel();
    }
    count
}

pub(super) fn record_revoked_parent(
    entries: &mut HashMap<String, Instant>,
    connection_id: &str,
    now: Instant,
) {
    prune_revoked_parents(entries, now);
    if entries.len() >= MAX_REVOKED_PARENTS {
        if let Some(oldest) = entries
            .iter()
            .min_by_key(|(_, revoked_at)| **revoked_at)
            .map(|(parent, _)| parent.clone())
        {
            entries.remove(&oldest);
        }
    }
    entries.insert(connection_id.to_string(), now);
}

pub(super) fn prune_revoked_parents(entries: &mut HashMap<String, Instant>, now: Instant) {
    entries.retain(|_, revoked_at| now.duration_since(*revoked_at) < REVOKED_PARENT_TTL);
}
