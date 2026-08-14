use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;
use crate::models::DbConversationDetail;

use super::conversation_history_cache_prune::{prune, remove_old_generations};

pub const HISTORY_PAGE_TURNS: usize = 120;
const HISTORY_CACHE_DIR: &str = "conversation-history";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PageMeta {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct HistoryCacheIndex {
    revision: String,
    pub(super) directory: String,
    total_turns: usize,
    #[serde(default)]
    transcript_watermark: Option<u64>,
    pages: Vec<PageMeta>,
}

fn write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn cache_dir() -> PathBuf {
    crate::system_skills::data_dir_from_env()
        .join("cache")
        .join(HISTORY_CACHE_DIR)
}

fn index_path(root: &Path, conversation_id: i32) -> PathBuf {
    root.join(format!("{conversation_id}.index.json"))
}

pub fn revision(external_id: Option<&str>, updated_at: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "{}:{}",
        external_id.unwrap_or_default(),
        updated_at.timestamp_millis()
    )
}

pub fn load(
    conversation_id: i32,
    expected_revision: &str,
    before: Option<usize>,
) -> Option<(DbConversationDetail, bool)> {
    let root = cache_dir();
    let index: HistoryCacheIndex =
        serde_json::from_slice(&fs::read(index_path(&root, conversation_id)).ok()?).ok()?;
    let page = select_page(&index.pages, before)?;
    let mut detail: DbConversationDetail = serde_json::from_slice(
        &fs::read(
            root.join(&index.directory)
                .join(format!("{}.json", page.start)),
        )
        .ok()?,
    )
    .ok()?;
    let fresh = index.revision == expected_revision;
    detail.history_stale = !fresh;
    Some((detail, fresh))
}

fn select_page(pages: &[PageMeta], before: Option<usize>) -> Option<&PageMeta> {
    match before {
        Some(end) => pages.iter().find(|page| page.end == end),
        None => pages.last(),
    }
}

pub fn page(detail: &DbConversationDetail, before: Option<usize>) -> DbConversationDetail {
    let total = detail.turns.len();
    let end = before.unwrap_or(total).min(total);
    let start = end.saturating_sub(HISTORY_PAGE_TURNS);
    page_detail(detail, start, end, total)
}

fn page_detail(
    detail: &DbConversationDetail,
    start: usize,
    end: usize,
    total: usize,
) -> DbConversationDetail {
    DbConversationDetail {
        summary: detail.summary.clone(),
        turns: detail.turns[start..end].to_vec(),
        session_stats: detail.session_stats.clone(),
        transcript_watermark: detail.transcript_watermark,
        in_flight_user_turn_id: detail.in_flight_user_turn_id.clone(),
        history_total_turns: total,
        history_start: start,
        history_assistant_turns_before: detail.turns[..start]
            .iter()
            .filter(|turn| matches!(turn.role, crate::models::TurnRole::Assistant))
            .count(),
        history_stale: false,
    }
}

pub fn store(conversation_id: i32, cache_revision: String, detail: DbConversationDetail) {
    let _guard = write_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Err(error) = store_with_file_lock(conversation_id, cache_revision, &detail) {
        tracing::warn!(
            conversation_id,
            error = %error,
            "[conversation-history] cache write failed"
        );
    }
}

fn store_with_file_lock(
    conversation_id: i32,
    cache_revision: String,
    detail: &DbConversationDetail,
) -> Result<(), AppCommandError> {
    let root = cache_dir();
    fs::create_dir_all(&root).map_err(AppCommandError::io)?;
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".write.lock"))
        .map_err(AppCommandError::io)?;
    lock.lock().map_err(AppCommandError::io)?;
    store_inner(&root, conversation_id, cache_revision, detail)
}

fn store_inner(
    root: &Path,
    conversation_id: i32,
    cache_revision: String,
    detail: &DbConversationDetail,
) -> Result<(), AppCommandError> {
    if newer_index_exists(root, conversation_id, &cache_revision, detail) {
        tracing::debug!(
            conversation_id,
            cache_revision,
            "[conversation-history] skipped stale cache writer"
        );
        return Ok(());
    }
    let generation = format!(
        "{conversation_id}-{}-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis(),
        uuid::Uuid::new_v4().simple()
    );
    let generation_dir = root.join(&generation);
    fs::create_dir_all(&generation_dir).map_err(AppCommandError::io)?;
    let pages = match write_pages(&generation_dir, detail) {
        Ok(pages) => pages,
        Err(error) => {
            let _ = fs::remove_dir_all(&generation_dir);
            return Err(error);
        }
    };
    let index = HistoryCacheIndex {
        revision: cache_revision,
        directory: generation.clone(),
        total_turns: detail.turns.len(),
        transcript_watermark: detail.transcript_watermark,
        pages,
    };
    if let Err(error) = persist_index(root, conversation_id, &index) {
        let _ = fs::remove_dir_all(&generation_dir);
        return Err(error);
    }
    remove_old_generations(root, conversation_id, &generation);
    prune(root);
    Ok(())
}

fn newer_index_exists(
    root: &Path,
    conversation_id: i32,
    revision: &str,
    detail: &DbConversationDetail,
) -> bool {
    let Ok(bytes) = fs::read(index_path(root, conversation_id)) else {
        return false;
    };
    let Ok(existing) = serde_json::from_slice::<HistoryCacheIndex>(&bytes) else {
        return false;
    };
    let existing_revision = revision_timestamp(&existing.revision);
    let incoming_revision = revision_timestamp(revision);
    if let Some((existing, incoming)) = existing_revision.zip(incoming_revision) {
        if existing != incoming {
            return existing > incoming;
        }
    }
    if existing.revision != revision {
        return false;
    }
    match (existing.transcript_watermark, detail.transcript_watermark) {
        (Some(existing), Some(incoming)) if existing != incoming => return existing > incoming,
        (Some(_), None) => return true,
        (None, Some(_)) => return false,
        _ => {}
    }
    existing.total_turns > detail.turns.len()
}

fn revision_timestamp(revision: &str) -> Option<i64> {
    revision.rsplit_once(':')?.1.parse().ok()
}

fn write_pages(
    generation_dir: &Path,
    detail: &DbConversationDetail,
) -> Result<Vec<PageMeta>, AppCommandError> {
    let total = detail.turns.len();
    let mut pages = Vec::new();
    let mut end = total;
    loop {
        let start = end.saturating_sub(HISTORY_PAGE_TURNS);
        let page = page_detail(detail, start, end, total);
        let bytes = serde_json::to_vec(&page)
            .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?;
        fs::write(generation_dir.join(format!("{start}.json")), bytes)
            .map_err(AppCommandError::io)?;
        pages.push(PageMeta { start, end });
        if start == 0 {
            break;
        }
        end = start;
    }
    pages.reverse();
    Ok(pages)
}

fn persist_index(
    root: &Path,
    conversation_id: i32,
    index: &HistoryCacheIndex,
) -> Result<(), AppCommandError> {
    let bytes = serde_json::to_vec(index)
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?;
    let mut temp = tempfile::NamedTempFile::new_in(root).map_err(AppCommandError::io)?;
    temp.write_all(&bytes).map_err(AppCommandError::io)?;
    temp.persist(index_path(root, conversation_id))
        .map_err(|error| AppCommandError::io(error.error))?;
    Ok(())
}
