//! 主动联系后的单次回答关联，不改变渠道原有会话绑定。
mod forward;
pub(crate) use forward::forward;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use super::types::{ChannelMessageTarget, IncomingCommand};

const REPLY_TTL: Duration = Duration::from_secs(600);
const MAX_REPLY_ROUTES: usize = 512;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ReplySource {
    pub connection_id: String,
    pub conversation_id: i32,
    pub turn_generation: i64,
}

#[derive(Clone)]
pub(super) struct ReplyRoute {
    pub id: String,
    pub source: ReplySource,
    channel_id: i32,
    target_id: String,
    sender_id: String,
    pub created_at: DateTime<Utc>,
    created: Instant,
}

#[derive(Default)]
pub(crate) struct ReplyRoutes(Mutex<Vec<ReplyRoute>>);

impl ReplyRoutes {
    pub(crate) fn register(
        &self,
        source: &ReplySource,
        target: (&str, &ChannelMessageTarget),
    ) -> Option<String> {
        let (target_id, target) = target;
        let sender_id = target.chat_id.as_ref().filter(|id| !id.is_empty())?;
        let mut routes = self.0.lock().unwrap_or_else(|error| error.into_inner());
        routes.retain(|route| route.created.elapsed() < REPLY_TTL);
        if routes.len() >= MAX_REPLY_ROUTES {
            tracing::warn!("[channel-reply] reply route capacity reached");
            return None;
        }
        let id = uuid::Uuid::new_v4().to_string();
        routes.push(ReplyRoute {
            id: id.clone(),
            source: source.clone(),
            channel_id: target.channel_id,
            target_id: target_id.to_string(),
            sender_id: sender_id.clone(),
            created_at: Utc::now(),
            created: Instant::now(),
        });
        tracing::info!(
            route_id = id,
            channel_id = target.channel_id,
            conversation_id = source.conversation_id,
            turn_generation = source.turn_generation,
            "[channel-reply] awaiting reply to original conversation"
        );
        Some(id)
    }

    pub(super) fn matching(&self, command: &IncomingCommand, target_id: &str) -> Vec<ReplyRoute> {
        let mut routes = self.0.lock().unwrap_or_else(|error| error.into_inner());
        routes.retain(|route| route.created.elapsed() < REPLY_TTL);
        let mut matches: Vec<ReplyRoute> = Vec::new();
        for route in routes.iter().rev().filter(|route| {
            route.channel_id == command.channel_id
                && route.target_id == target_id
                && route.sender_id == command.sender_id
                && command.received_at >= route.created_at
        }) {
            if !matches.iter().any(|matched| matched.source == route.source) {
                matches.push(route.clone());
            }
        }
        matches
    }

    pub(crate) fn remove(&self, id: &str) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|route| route.id != id);
    }

    pub(super) fn finish(&self, source: &ReplySource) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|route| route.source != *source);
    }

    pub(super) fn consume(&self, source: &ReplySource, before: Instant) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|route| route.source != *source || route.created > before);
    }

    pub(crate) fn remove_channel(&self, channel_id: i32) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|route| route.channel_id != channel_id);
    }
}
