use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::acp::types::{AcpEvent, EventEnvelope, ToolCallImageInfo, UserMessageBlock};

/// Capacity of the per-connection broadcast channel. Sized to absorb a brief
/// burst when a slow subscriber lags; broadcast::channel drops oldest events
/// past capacity (RecvError::Lagged), which the subscriber surfaces as a
/// `replay_lagged` cue and the client converts to a re-attach.
const BROADCAST_CAPACITY: usize = 4096;

/// Maximum byte total retained in the recent-events ring buffer. Sized so
/// even an active streaming session with several tool-call updates fits
/// comfortably; oversized images push past this bound and force a snapshot
/// fallback on the next attach (see `RecentEventsBuffer::push`).
pub const RECENT_BUFFER_MAX_BYTES: usize = 128 * 1024;

/// Hard cap on event count regardless of byte total. Defends against a
/// pathological flood of tiny events filling the buffer past the byte limit
/// (each event has a small overhead — connection_id, seq — that doesn't
/// contribute meaningfully to byte_total but does to memory).
pub const RECENT_BUFFER_MAX_COUNT: usize = 128;

/// Single-event size threshold above which we refuse to store the event.
/// Stored events would be replayed on reconnect; an oversized event blows
/// past WS frame budgets. The next attach for such a connection will fall
/// through to a snapshot, which is the right thing for large state.
const RECENT_EVENT_MAX_BYTES: usize = 64 * 1024;

/// Per-connection event broadcaster + recent-events ring buffer.
///
/// Lives on `SessionState` (one per active ACP connection). All event
/// emission for a connection goes through `emit_with_state`, which holds
/// the SessionState write lock while:
///   1. applying the event
///   2. incrementing event_seq
///   3. pushing the resulting envelope into `recent_events`
///
/// then releases the lock and broadcasts via `sender`.
///
/// New WS subscribers (`attach`) hold the SessionState **read** lock while:
///   1. snapshotting the state and event_seq
///   2. (optionally) reading recent_events for replay
///   3. calling `subscribe()` on this stream
///
/// then release the lock.
///
/// Holding the read lock across subscribe() guarantees no event broadcast
/// races between the snapshot read and receiver registration: the only
/// path that produces broadcasts is `emit_with_state`, which needs the
/// write lock and therefore waits.
#[derive(Debug)]
pub struct ConnectionEventStream {
    sender: broadcast::Sender<Arc<EventEnvelope>>,
}

impl Default for ConnectionEventStream {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionEventStream {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self { sender }
    }

    /// Register a new subscriber. Must be called while holding at least a
    /// read lock on the owning `SessionState`, otherwise events emitted
    /// after the snapshot read but before subscribe can be missed.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<EventEnvelope>> {
        self.sender.subscribe()
    }

    /// Broadcast an envelope. Failure (no subscribers) is ignored — the
    /// event is already recorded in `SessionState.recent_events` for the
    /// next attach to pick up via replay.
    pub fn send(&self, envelope: Arc<EventEnvelope>) {
        let _ = self.sender.send(envelope);
    }
}

/// Bounded ring buffer of recent events, used to replay short reconnect
/// gaps without forcing a full snapshot. Two limits are enforced together:
/// `MAX_BYTES` (network/memory) and `MAX_COUNT` (defense-in-depth against
/// many tiny events).
#[derive(Debug)]
pub struct RecentEventsBuffer {
    events: VecDeque<RecentEntry>,
    byte_total: usize,
}

#[derive(Debug)]
struct RecentEntry {
    seq: u64,
    size: usize,
    envelope: Arc<EventEnvelope>,
}

impl Default for RecentEventsBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl RecentEventsBuffer {
    pub fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(32),
            byte_total: 0,
        }
    }

    /// Push an envelope. If estimated size exceeds the per-event limit, the
    /// envelope is silently skipped — an attach with a cursor pointing at
    /// or before this seq will detect the gap and fall back to a snapshot.
    ///
    /// Returns the number of events evicted by this push (FIFO eviction
    /// triggered by either count cap or byte cap, plus the wholesale clear
    /// for oversized events). Callers wire this into `EventBusMetrics::
    /// ring_buffer_evict_count` so operators can detect ring-buffer pressure.
    #[must_use = "evicted count feeds the ring_buffer_evict_count metric"]
    pub fn push(&mut self, envelope: Arc<EventEnvelope>) -> usize {
        let size = estimate_envelope_size(&envelope);
        if size > RECENT_EVENT_MAX_BYTES {
            // Mark the gap implicitly: the next event will appear non-contiguous
            // relative to its predecessor, and `range_after` returns None.
            // Drop the entire buffer so a subsequent attach with an old cursor
            // takes the snapshot path rather than returning a misleading
            // partial replay.
            let evicted = self.events.len();
            self.events.clear();
            self.byte_total = 0;
            return evicted;
        }
        let seq = envelope.seq;
        self.events.push_back(RecentEntry {
            seq,
            size,
            envelope,
        });
        self.byte_total = self.byte_total.saturating_add(size);
        let mut evicted = 0;
        while self.events.len() > RECENT_BUFFER_MAX_COUNT
            || self.byte_total > RECENT_BUFFER_MAX_BYTES
        {
            match self.events.pop_front() {
                Some(old) => {
                    self.byte_total = self.byte_total.saturating_sub(old.size);
                    evicted += 1;
                }
                None => break,
            }
        }
        evicted
    }

    /// Returns events with seq strictly greater than `since_seq`, in order.
    /// `None` indicates the cursor is older than the oldest buffered seq —
    /// caller must fall back to a snapshot rather than send partial replay.
    pub fn range_after(&self, since_seq: u64) -> Option<Vec<Arc<EventEnvelope>>> {
        let oldest = self.events.front()?.seq;
        // since_seq + 1 is the first seq we'd want; if our oldest is past
        // that, there's a gap we can't fill.
        if oldest > since_seq.saturating_add(1) {
            return None;
        }
        Some(
            self.events
                .iter()
                .filter(|e| e.seq > since_seq)
                .map(|e| e.envelope.clone())
                .collect(),
        )
    }
}

/// Serialized-JSON length of a string: its UTF-8 byte length plus the extra
/// bytes JSON escaping adds (the two surrounding quotes, `\"`, `\\`, and
/// control-char escapes), computed WITHOUT allocating. Escape-awareness matters
/// because this feeds the per-event size cap: an escape-heavy payload (tool
/// output full of quotes/newlines, say) serializes much larger than its raw byte
/// length and must still be recognized as oversized.
fn json_str_len(s: &str) -> usize {
    let mut extra = 0usize;
    for b in s.bytes() {
        match b {
            // `"`, `\`, and the short control escapes (\b \t \n \f \r) each
            // serialize to two bytes → one extra byte over the raw byte.
            b'"' | b'\\' | 0x08 | 0x09 | 0x0A | 0x0C | 0x0D => extra += 1,
            // Any other control char serializes as `\u00XX` (six bytes) → +5.
            c if c < 0x20 => extra += 5,
            _ => {}
        }
    }
    // + 2 for the surrounding quotes.
    2 + s.len() + extra
}

/// Decimal digit count of `n` (0 → 1), without allocating.
fn decimal_len(n: u64) -> usize {
    if n == 0 {
        1
    } else {
        (n.ilog10() as usize) + 1
    }
}

/// Serialized length of a JSON number, without allocating. Integers are sized by
/// exact digit count (plus a sign byte); non-integers (f64) fall back to a
/// conservative upper bound — serde_json prints an f64 to at most ~24 bytes — so
/// a number-dense payload is never undercounted.
fn number_size(n: &serde_json::Number) -> usize {
    if let Some(u) = n.as_u64() {
        decimal_len(u)
    } else if let Some(i) = n.as_i64() {
        1 + decimal_len(i.unsigned_abs())
    } else {
        24
    }
}

/// Cheap byte estimate for a JSON value's footprint — sums (escape-aware) string
/// and key lengths, numbers, and the structural punctuation JSON serialization
/// adds: brackets/braces, the `:` after each key, and the `,` BETWEEN elements.
/// Computed without serializing and never undercounting, so it stays a safe
/// proxy for the per-event size cap even for dense arrays/objects.
fn json_value_size(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Null => 4,
        serde_json::Value::Bool(b) => {
            if *b {
                4
            } else {
                5
            }
        }
        serde_json::Value::Number(n) => number_size(n),
        serde_json::Value::String(s) => json_str_len(s),
        serde_json::Value::Array(items) => {
            // `[` + elements + `,` between them + `]`.
            2 + items.len().saturating_sub(1) + items.iter().map(json_value_size).sum::<usize>()
        }
        serde_json::Value::Object(map) => {
            // `{` + `"key":value` pairs + `,` between them + `}`.
            2 + map.len().saturating_sub(1)
                + map
                    .iter()
                    .map(|(k, v)| json_str_len(k) + 1 + json_value_size(v))
                    .sum::<usize>()
        }
    }
}

fn opt_str_size(s: &Option<String>) -> usize {
    s.as_ref().map_or(0, |v| json_str_len(v))
}

fn opt_json_size(v: &Option<serde_json::Value>) -> usize {
    v.as_ref().map_or(0, json_value_size)
}

/// Sum the payload of any attached images: the `[` `]` brackets, each image's
/// `{"data":"..","mime_type":".."[,"uri":".."]}` object structure, and its
/// fields. `data` (the base64 image, the dominant term) is sized escape-aware
/// like every other string — for valid base64 that is just its byte length plus
/// the two quotes, but `data` is a plain `String`, so sizing it defensively
/// rather than by raw `len()` keeps the `estimate >= serialized` invariant true
/// even if a producer put JSON-escapable bytes in the field (else an oversized
/// image could slip under the per-event cap). This does scan `data`, but with no
/// allocation — far cheaper than the full-envelope `serde_json::to_vec` this
/// replaced — and image events are infrequent (not on the per-token path).
fn images_size(images: &Option<Vec<ToolCallImageInfo>>) -> usize {
    images.as_ref().map_or(0, |imgs| {
        // `PER_IMAGE_STRUCT` conservatively bounds each object's keys/braces and
        // the trailing comma; `+ 2` is the array brackets.
        const PER_IMAGE_STRUCT: usize = 48;
        2 + imgs
            .iter()
            .map(|img| {
                PER_IMAGE_STRUCT
                    + json_str_len(&img.data)
                    + json_str_len(&img.mime_type)
                    + opt_str_size(&img.uri)
            })
            .sum::<usize>()
    })
}

/// Byte size of a single user-message block, including its `{"type":..,..}`
/// object structure (keys/braces) as a small conservative constant on top of the
/// escape-aware value bytes (image `data` sized like `images_size`).
fn user_block_size(block: &UserMessageBlock) -> usize {
    match block {
        // `{"type":"text","text":<v>}`
        UserMessageBlock::Text { text } => 24 + json_str_len(text),
        // `{"type":"image","data":"<d>","mime_type":<v>}`
        UserMessageBlock::Image { data, mime_type } => {
            48 + json_str_len(data) + json_str_len(mime_type)
        }
    }
}

/// Best-effort byte estimate for an event envelope's footprint in the recent-
/// events ring buffer. Feeds BOTH the running byte cap and the per-event
/// `RECENT_EVENT_MAX_BYTES` threshold, so it must track the serialized size
/// closely enough that oversized events (large tool output, base64 images) still
/// trip the per-event cap and force a snapshot fallback — hence the escape-aware
/// string sizing (see `json_str_len`).
///
/// `emit_with_state` calls this on every event while holding the `SessionState`
/// write lock. The hot, high-frequency, and potentially-large variants —
/// streaming text/thinking deltas, tool calls and updates and user messages
/// (which can carry multi-MB base64 images), and forwarded Claude SDK messages —
/// are therefore estimated STRUCTURALLY from their string/JSON fields, with no
/// serialization: serializing a per-token delta or a multi-MB image on that
/// locked hot path only to measure and discard the bytes was the cost this
/// replaced. Every other variant is small and infrequent, so it falls back to an
/// exact serialized length — cheap here, faithful to the prior sizing, and
/// needing no upkeep as variants are added.
fn estimate_envelope_size(envelope: &EventEnvelope) -> usize {
    // Conservative fixed overhead: the envelope skeleton (`{"seq":N,...}`), the
    // `type` tag, and every structural variant's field KEYS/colons plus the
    // `null`s that its non-skipped `Option::None` fields serialize to. It must
    // exceed the largest structural variant's fixed serialized overhead
    // (ToolCall / ToolCallUpdate: ~190 B with a 20-digit seq and several `null`
    // fields) so `base + payload` NEVER undercounts the serialized envelope — the
    // invariant the per-event cap relies on to reject oversized events, asserted
    // for every structural branch by `estimate_never_undercounts_serialized_*`.
    // Over-counting small events is harmless: streaming deltas hit the count cap
    // long before this matters, and it is negligible against a large payload.
    const ENVELOPE_OVERHEAD: usize = 256;
    // `connection_id` is sized escape-aware like every other string so the
    // `estimate >= serialized` invariant holds for ANY id, not just the
    // UUID-shaped ones production emits. (`ENVELOPE_OVERHEAD` covers its key.)
    let base = ENVELOPE_OVERHEAD + json_str_len(&envelope.connection_id);
    let payload = match &envelope.payload {
        AcpEvent::ContentDelta { text } | AcpEvent::Thinking { text } => json_str_len(text),
        AcpEvent::ClaudeSdkMessage {
            session_id,
            message,
        } => json_str_len(session_id) + json_value_size(message),
        AcpEvent::ToolCall {
            tool_call_id,
            title,
            kind,
            status,
            content,
            raw_input,
            raw_output,
            locations,
            meta,
            images,
        } => {
            json_str_len(tool_call_id)
                + json_str_len(title)
                + json_str_len(kind)
                + json_str_len(status)
                + opt_str_size(content)
                + opt_str_size(raw_input)
                + opt_str_size(raw_output)
                + opt_json_size(locations)
                + opt_json_size(meta)
                + images_size(images)
        }
        AcpEvent::ToolCallUpdate {
            tool_call_id,
            title,
            status,
            content,
            raw_input,
            raw_output,
            locations,
            meta,
            images,
            // Spelled out (not `..`) so a newly-added large field forces this
            // estimator to be revisited rather than silently under-counted.
            raw_output_append: _,
        } => {
            json_str_len(tool_call_id)
                + opt_str_size(title)
                + opt_str_size(status)
                + opt_str_size(content)
                + opt_str_size(raw_input)
                + opt_str_size(raw_output)
                + opt_json_size(locations)
                + opt_json_size(meta)
                + images_size(images)
        }
        // Can carry a base64 `UserMessageBlock::Image` from a pasted prompt
        // image, so it is sized structurally too — otherwise a multi-MB user
        // image would be fully serialized under the write lock via the fallback.
        AcpEvent::UserMessage { message_id, blocks } => {
            // `"blocks":[` + block objects + `,` between them + `]` (the
            // `message_id`/`blocks` keys themselves are covered by the base).
            json_str_len(message_id)
                + 2
                + blocks.len().saturating_sub(1)
                + blocks.iter().map(user_block_size).sum::<usize>()
        }
        AcpEvent::BackgroundActivity {
            session_id,
            turns,
            outstanding: _,
            settled,
            watermark: _,
        } => {
            json_str_len(session_id)
                + turns.len().saturating_sub(1)
                + turns.iter().map(message_turn_size).sum::<usize>()
                + settled
                    .iter()
                    .map(|item| {
                        128 + json_str_len(&item.task_id)
                            + json_str_len(&item.status)
                            + opt_str_size(&item.summary)
                            + opt_str_size(&item.tool_use_id)
                            + opt_str_size(&item.result)
                    })
                    .sum::<usize>()
        }
        // Small, infrequent variants: an exact serialized length is cheap here
        // and preserves the prior threshold behavior; the 256 fallback only
        // guards the (practically impossible) serialization failure.
        other => serde_json::to_vec(other).map_or(256, |v| v.len()),
    };
    base + payload
}

fn message_turn_size(turn: &crate::models::message::MessageTurn) -> usize {
    384 + json_str_len(&turn.id)
        + opt_str_size(&turn.model)
        + turn.blocks.len().saturating_sub(1)
        + turn.blocks.iter().map(content_block_size).sum::<usize>()
}

fn content_block_size(block: &crate::models::message::ContentBlock) -> usize {
    use crate::models::message::ContentBlock as Block;

    match block {
        Block::Text { text } | Block::Thinking { text } => 32 + json_str_len(text),
        Block::Image {
            data,
            mime_type,
            uri,
        } => 64 + json_str_len(data) + json_str_len(mime_type) + opt_str_size(uri),
        Block::ImageGeneration {
            revised_prompt,
            image,
        } => {
            64 + opt_str_size(revised_prompt)
                + image.as_ref().map_or(0, |item| {
                    64 + json_str_len(&item.data)
                        + json_str_len(&item.mime_type)
                        + opt_str_size(&item.uri)
                })
        }
        Block::ToolUse {
            tool_use_id,
            tool_name,
            input_preview,
            meta,
        } => {
            96 + opt_str_size(tool_use_id)
                + json_str_len(tool_name)
                + opt_str_size(input_preview)
                + opt_json_size(meta)
        }
        Block::ToolResult {
            tool_use_id,
            output_preview,
            is_error: _,
            agent_stats,
            images,
        } => {
            128 + opt_str_size(tool_use_id)
                + opt_str_size(output_preview)
                + agent_stats.as_ref().map_or(0, agent_stats_size)
                + images
                    .iter()
                    .map(|item| {
                        64 + json_str_len(&item.data)
                            + json_str_len(&item.mime_type)
                            + opt_str_size(&item.uri)
                    })
                    .sum::<usize>()
        }
    }
}

fn agent_stats_size(stats: &crate::models::message::AgentExecutionStats) -> usize {
    640 + opt_str_size(&stats.agent_type)
        + opt_str_size(&stats.status)
        + stats
            .tool_calls
            .iter()
            .map(|call| {
                96 + json_str_len(&call.tool_name)
                    + opt_str_size(&call.input_preview)
                    + opt_str_size(&call.output_preview)
            })
            .sum::<usize>()
}
