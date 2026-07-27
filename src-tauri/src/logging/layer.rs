//! The custom `tracing` layer that feeds the in-memory ring buffer and the
//! live-tail event stream. Runs alongside the stderr fmt layer and the JSON
//! file layer in the subscriber stack ([`crate::logging::init`]).
//!
//! It captures, per event: the `message`, the event's other key-value fields,
//! and the enclosing span chain (each span's name + recorded fields). Span
//! fields are stashed in the span's `extensions` at creation ([`on_new_span`])
//! and updated by later `Span::record` calls ([`on_record`]); [`on_event`]
//! walks the scope root→leaf to assemble them. Plain-message logs (no fields,
//! no span) produce empty `fields`/`spans` and render exactly as before.
//!
//! ## Instrumenting new call sites
//!
//! To make logs correlatable, annotate async functions with
//! `#[instrument(name = "...", skip_all, fields(connection_id = %id, ...))]`
//! using only the canonical correlators (connection_id, conversation_id,
//! task_id, agent_type). For a spawned loop (not an `async fn`), build a
//! per-iteration `info_span!(...)` and `.instrument(span)` the body — never
//! `let _g = span.enter()` across an `.await`, which corrupts the span stack.

use std::cell::Cell;
use std::collections::BTreeMap;

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

use crate::logging::hub::{log_hub, LogRecord, SpanInfo};

thread_local! {
    /// Reentrancy guard for ALL layer callbacks. The emit path in
    /// `LogHub::record` (broadcast + serde) — and a custom `Debug`/`Display`
    /// that itself logs while we format a field — can transitively call
    /// `tracing::*`; without this, that would re-enter the layer (and, in
    /// `on_event`, `record` → emit → … unbounded) on the same thread.
    static IN_LAYER: Cell<bool> = const { Cell::new(false) };
}

/// RAII reentrancy guard. [`LayerGuard::enter`] sets the thread-local flag and
/// returns `Some` only when it was previously unset; a nested call gets `None`
/// and bails. Resets on drop (panic-safe). Used by every `Layer` callback so
/// field formatting can never re-enter the layer on the same thread.
struct LayerGuard;

impl LayerGuard {
    fn enter() -> Option<Self> {
        if IN_LAYER.with(|f| f.replace(true)) {
            None
        } else {
            Some(LayerGuard)
        }
    }
}

impl Drop for LayerGuard {
    fn drop(&mut self) {
        IN_LAYER.with(|f| f.set(false));
    }
}

/// Captures an event's (or span's) fields: the `message` field is split out;
/// every other field becomes a `key → value` string entry. Mirrors the human
/// rendering — `%`/`?`/literal/number/bool all collapse to their text form.
#[derive(Default)]
struct FieldVisitor {
    message: Option<String>,
    fields: BTreeMap<String, String>,
}

impl FieldVisitor {
    fn put(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = Some(value);
        } else {
            self.fields.insert(field.name().to_string(), value);
        }
    }

    /// Collapse to a field map for SPAN context, keeping any `message` field as
    /// a normal field — only events have a special "message"; a span field
    /// literally named `message` must not be dropped.
    fn into_span_fields(mut self) -> BTreeMap<String, String> {
        if let Some(msg) = self.message.take() {
            self.fields.insert("message".to_string(), msg);
        }
        self.fields
    }
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.put(field, format!("{value:?}"));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.put(field, value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, value.to_string());
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.put(field, value.to_string());
    }
}

/// Per-span recorded fields, stored in the span's `extensions` (keyed by type,
/// the same mechanism `fmt`'s `FormattedFields` uses).
struct SpanFields(BTreeMap<String, String>);

/// Layer that converts each event into a [`LogRecord`] and hands it to the
/// global [`LogHub`]. A no-op until the hub is installed (so `iyw-claw-mcp`, which
/// installs no hub, pays nothing). Requires `LookupSpan` so it can read span
/// context from the registry.
pub struct BufferEmitLayer;

impl<S> Layer<S> for BufferEmitLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        // Guarded so a field's custom Debug that logs can't re-enter the layer,
        // and so a span opened by the emit path (guard already set) is skipped.
        let Some(_guard) = LayerGuard::enter() else {
            return;
        };
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut visitor = FieldVisitor::default();
        attrs.record(&mut visitor);
        span.extensions_mut()
            .insert(SpanFields(visitor.into_span_fields()));
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(_guard) = LayerGuard::enter() else {
            return;
        };
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut visitor = FieldVisitor::default();
        values.record(&mut visitor);
        let new_fields = visitor.into_span_fields();
        let mut ext = span.extensions_mut();
        if let Some(existing) = ext.get_mut::<SpanFields>() {
            // Later writes override (e.g. a `field::Empty` filled in later).
            existing.0.extend(new_fields);
        } else {
            ext.insert(SpanFields(new_fields));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        // Same-thread reentry from the emit path (or field formatting) → no-op.
        let Some(_guard) = LayerGuard::enter() else {
            return;
        };

        let Some(hub) = log_hub() else {
            return;
        };

        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        // Walk the enclosing span scope root→leaf. Clone each span's fields out
        // inside the loop and drop the `extensions()` borrow before recording —
        // never hold an extensions guard across `hub.record()`.
        let mut spans = Vec::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let fields = span
                    .extensions()
                    .get::<SpanFields>()
                    .map(|sf| sf.0.clone())
                    .unwrap_or_default();
                spans.push(SpanInfo {
                    name: span.name().to_string(),
                    fields,
                });
            }
        }

        let meta = event.metadata();
        hub.record(LogRecord {
            seq: hub.next_seq(),
            timestamp_ms: now_ms(),
            level: meta.level().as_str(),
            target: meta.target().to_string(),
            message: visitor.message.unwrap_or_default(),
            fields: visitor.fields,
            spans,
        });
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

