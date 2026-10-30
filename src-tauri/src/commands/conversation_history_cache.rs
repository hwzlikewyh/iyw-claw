use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;
use crate::models::DbConversationDetail;

use super::conversation_history_cache_prune::{prune, remove_old_generations};

mod scheduling;
pub(super) use scheduling::schedule;
mod reading;
pub use reading::load;

pub const HISTORY_PAGE_TURNS: usize = 120;
// 一次翻页补齐可见轮次，同时限制超长工具回合的单次内存和传输量。
const MAX_HISTORY_PAGE_TURNS: usize = HISTORY_PAGE_TURNS * 8;
const HISTORY_CACHE_DIR: &str = "conversation-history";
const HISTORY_CACHE_PARSER_REVISION: u8 = 11;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PageMeta {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct HistoryCacheIndex {
    revision: String,
    #[serde(default)]
    parser_revision: u8,
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

pub fn page(detail: &DbConversationDetail, before: Option<usize>) -> DbConversationDetail {
    let total = detail.turns.len();
    let end = before.unwrap_or(total).min(total);
    let mut start = end.saturating_sub(HISTORY_PAGE_TURNS);
    if before.is_some() {
        start = earlier_start(&detail.turns, start..end);
    }
    let assistants_before = count_assistants(&detail.turns[..start]);
    let mut page = page_detail(detail, start..end, assistants_before);
    // 翻页只传正文，沿用首屏已按当前账号核对的账单。
    if before.is_none() {
        page.backend_consumption = detail.backend_consumption.clone();
    }
    page
}

fn earlier_start(turns: &[crate::models::MessageTurn], range: std::ops::Range<usize>) -> usize {
    let is_user =
        |turn: &crate::models::MessageTurn| matches!(turn.role, crate::models::TurnRole::User);
    if let Some(offset) = turns[range.clone()].iter().position(is_user) {
        return range.start + offset;
    }
    let limit = range.end.saturating_sub(MAX_HISTORY_PAGE_TURNS);
    turns[limit..range.start]
        .iter()
        .rposition(is_user)
        .map_or(limit, |offset| limit + offset)
}

fn page_detail(
    detail: &DbConversationDetail,
    range: std::ops::Range<usize>,
    assistants_before: usize,
) -> DbConversationDetail {
    DbConversationDetail {
        backend_consumption: None,
        summary: detail.summary.clone(),
        turns: detail.turns[range.clone()].to_vec(),
        session_stats: detail.session_stats.clone(),
        transcript_watermark: detail.transcript_watermark,
        in_flight_user_turn_id: detail.in_flight_user_turn_id.clone(),
        history_total_turns: detail.turns.len(),
        history_start: range.start,
        history_assistant_turns_before: assistants_before,
        history_stale: false,
    }
}

fn count_assistants(turns: &[crate::models::MessageTurn]) -> usize {
    turns
        .iter()
        .filter(|turn| matches!(turn.role, crate::models::TurnRole::Assistant))
        .count()
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
        parser_revision: HISTORY_CACHE_PARSER_REVISION,
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
    if existing.parser_revision != HISTORY_CACHE_PARSER_REVISION {
        return false;
    }
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
    let mut assistants_before = count_assistants(&detail.turns);
    loop {
        let start = end.saturating_sub(HISTORY_PAGE_TURNS);
        assistants_before -= count_assistants(&detail.turns[start..end]);
        let page = page_detail(detail, start..end, assistants_before);
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
