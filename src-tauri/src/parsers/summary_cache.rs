//! Process-global cache for per-file conversation summaries.
//!
//! `list_conversations` (and, through it, `list_folders`, `get_stats`, and
//! `get_sidebar_data`) rebuilds the whole history list on every request by
//! walking each agent's session tree and parsing every file to EOF — the
//! per-file summary parse can't early-exit because it needs the *last*
//! timestamp and the total message count. With four hot endpoints and a history
//! that only grows, that re-parse cost is unbounded.
//!
//! This module memoizes a `ConversationSummary` per `(AgentType, path)`,
//! invalidated by a cheap `(mtime, size)` fingerprint: a cache hit `stat`s the
//! file (microseconds) instead of reading and JSON-parsing it (milliseconds-to-
//! seconds for a long session). The one actively-streaming file re-parses on
//! each list — every other (dormant) conversation is served from cache.
//!
//! ## What may route through here
//!
//! Only parsers whose cached summary is a *pure function of one file's bytes*:
//! Claude, Codex, CodeBuddy, and Pi each derive the summary they hand to the
//! cache from the single file being parsed (`folder_path` comes from an in-file
//! `cwd`). Claude additionally fills a *missing* `folder_path` from the file's
//! parent-directory name, but that fallback runs in the list loop AFTER the
//! cache returns, on the returned clone — it's deterministic given the file's
//! location and never mutates the cached entry, so it doesn't affect validity.
//!
//! The key is namespaced by `AgentType`, not path alone: the cache is shared by
//! every parser, and a value depends on WHICH parser produced it. If two agents'
//! roots are configured to overlap (distinct env overrides normally keep them
//! apart, but nothing enforces it), a bare-path key would let one parser serve
//! another parser's summary for the same file — wrong `agent_type` and fields.
//!
//! Deliberately NOT cached:
//! - **Gemini** — its `folder_path` is resolved from external `.project_root` /
//!   `projects.json` files, so the summary is *not* a pure function of the chat
//!   file; an (mtime, size) key on the chat file alone could serve a stale
//!   folder after that external metadata changes.
//! - **Cline / OpenClaw / Kimi** — a conversation spans multiple files, or one
//!   file yields many summaries plus external index state; a single path
//!   fingerprint is insufficient.
//! - **OpenCode / Hermes** — SQLite-backed; already query an indexed store.
//!
//! ## Invalidation token
//!
//! `(mtime, size)`. Size alone catches every append (the jsonl session logs only
//! ever grow); mtime additionally catches a same-size in-place rewrite whenever
//! the clock advanced. The residual blind spot — a same-size rewrite that also
//! preserves mtime (`cp -p`, a timestamp-preserving restore, or coarse
//! filesystem mtime granularity within the same tick) — is accepted: these are
//! append-only agent logs, not hand-edited files, and the app's import/restore
//! path writes the SQLite DB rather than rewriting these logs in place with
//! preserved timestamps. As an extra guard, an entry is only cached when the
//! file's fingerprint is unchanged across the parse (see `get_or_parse`), so a
//! write that races the read — including the actively-streaming file — is not
//! memoized under a stale summary.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use super::ParseError;
use crate::models::{AgentType, ConversationSummary};

/// File-content fingerprint: `(mtime, size)`. `mtime` is `Option` because a
/// platform may not report it; a `None`/`None` comparison then falls back to
/// size-only.
type Fingerprint = (Option<SystemTime>, u64);

struct CacheEntry {
    fingerprint: Fingerprint,
    /// Only positive summaries are stored — see `get_or_parse`.
    summary: ConversationSummary,
}

/// Nested `AgentType → (path → entry)` so a lookup borrows `&Path` (no
/// per-lookup `PathBuf` allocation on the hot hit path) while still namespacing
/// by parser identity.
type Cache = HashMap<AgentType, HashMap<PathBuf, CacheEntry>>;

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Stat the file for its `(mtime, size)` fingerprint. Returns `None` (⇒ bypass
/// the cache) when the file can't be stat'd.
fn fingerprint(path: &Path) -> Option<Fingerprint> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok(), meta.len()))
}

/// Return the cached summary for `(agent_type, path)` if the file is unchanged
/// since it was last parsed; otherwise run `parse`, cache a positive result, and
/// return it.
///
/// `parse` is invoked at most once per call and runs OUTSIDE the cache lock — it
/// reads and parses the whole file, which is the expensive work being cached, so
/// holding the map lock across it would serialize every concurrent list request.
/// A concurrent miss on the same key may therefore parse twice; that's harmless
/// (the parse is idempotent) and rare.
///
/// `agent_type` namespaces the key: the cache is shared by all parsers and the
/// summary depends on which parser produced it, so two parsers scanning the same
/// path (overlapping roots) must not share an entry.
///
/// Caching rules, chosen so a cache entry can never *hide* a real conversation:
/// - Only `Ok(Some(_))` is stored. `Ok(None)` (an empty / not-a-conversation
///   file — for some parsers also a transient `File::open` failure that
///   degrades to `None`) and `Err` are returned but never cached, so they
///   re-parse next time and self-heal. Re-parsing a `None` file is cheap: it has
///   no content to read.
/// - A positive result is stored only if the file's fingerprint is unchanged
///   across the parse. If the file was written while we read it (a torn read, or
///   the actively-streaming session file), we return the parse but don't cache
///   it, so the next list re-parses the settled bytes.
pub(crate) fn get_or_parse<F>(
    agent_type: AgentType,
    path: &Path,
    parse: F,
) -> Result<Option<ConversationSummary>, ParseError>
where
    F: FnOnce() -> Result<Option<ConversationSummary>, ParseError>,
{
    // Can't fingerprint (file vanished, permission error, …) ⇒ bypass the cache:
    // parse fresh and store nothing, since there's no token to validate against.
    let Some(fp_before) = fingerprint(path) else {
        return parse();
    };

    // Fast path: a live fingerprint match returns the cached summary without
    // reading or parsing the file.
    if let Ok(map) = cache().lock() {
        if let Some(entry) = map.get(&agent_type).and_then(|m| m.get(path)) {
            if entry.fingerprint == fp_before {
                return Ok(Some(entry.summary.clone()));
            }
        }
    }

    // Miss or stale: parse with the lock released.
    let parsed = parse()?;

    // Cache only a positive result, and only if the file didn't change while we
    // read it (`fingerprint(path) == fp_before`). A changed fingerprint means a
    // concurrent write raced the read (or this is the streaming file); leave it
    // uncached so the next list re-parses the settled content.
    if let Some(summary) = &parsed {
        if fingerprint(path) == Some(fp_before) {
            if let Ok(mut map) = cache().lock() {
                map.entry(agent_type).or_default().insert(
                    path.to_path_buf(),
                    CacheEntry {
                        fingerprint: fp_before,
                        summary: summary.clone(),
                    },
                );
            }
        }
    }
    Ok(parsed)
}

