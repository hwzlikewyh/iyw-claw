use std::fs;
use std::ops::Range;
use std::path::Path;

use crate::models::{
    DbConversationDetail, DbConversationSummary, MessageTurn, SessionStats, TurnRole,
};

use super::{
    cache_dir, count_assistants, index_path, HistoryCacheIndex, PageMeta,
    HISTORY_CACHE_PARSER_REVISION, HISTORY_PAGE_TURNS, MAX_HISTORY_PAGE_TURNS,
};

struct HistoryRead<'a> {
    conversation_id: i32,
    expected_revision: &'a str,
    before: Option<usize>,
}

struct CacheReader<'a> {
    root: &'a Path,
    index: &'a HistoryCacheIndex,
}

#[derive(serde::Deserialize)]
struct CachedPage {
    #[serde(rename = "backend_consumption")]
    _backend_consumption: Option<serde::de::IgnoredAny>,
    summary: DbConversationSummary,
    turns: Vec<MessageTurn>,
    session_stats: Option<SessionStats>,
    transcript_watermark: Option<u64>,
    in_flight_user_turn_id: Option<String>,
    #[serde(default)]
    history_total_turns: usize,
    #[serde(default)]
    history_start: usize,
    #[serde(default)]
    history_assistant_turns_before: usize,
    #[serde(default)]
    history_stale: bool,
}

impl From<CachedPage> for DbConversationDetail {
    fn from(page: CachedPage) -> Self {
        Self {
            backend_consumption: None,
            summary: page.summary,
            turns: page.turns,
            session_stats: page.session_stats,
            transcript_watermark: page.transcript_watermark,
            in_flight_user_turn_id: page.in_flight_user_turn_id,
            history_total_turns: page.history_total_turns,
            history_start: page.history_start,
            history_assistant_turns_before: page.history_assistant_turns_before,
            history_stale: page.history_stale,
        }
    }
}

pub fn load(
    conversation_id: i32,
    expected_revision: &str,
    before: Option<usize>,
) -> Option<(DbConversationDetail, bool)> {
    let root = cache_dir();
    let request = HistoryRead {
        conversation_id,
        expected_revision,
        before,
    };
    let read = || load_once(&root, &request);
    // 写入器发布新索引后会清理旧目录；发生竞争时重读索引，避免退回全文解析。
    read().or_else(read)
}

fn load_once(root: &Path, request: &HistoryRead<'_>) -> Option<(DbConversationDetail, bool)> {
    let index: HistoryCacheIndex =
        serde_json::from_slice(&fs::read(index_path(root, request.conversation_id)).ok()?).ok()?;
    if index.parser_revision != HISTORY_CACHE_PARSER_REVISION
        || index.revision.rsplit_once(':')?.0 != request.expected_revision.rsplit_once(':')?.0
    {
        return None;
    }
    let end = request.before.unwrap_or(index.total_turns);
    if end > index.total_turns {
        return None;
    }
    let range = end.saturating_sub(HISTORY_PAGE_TURNS)..end;
    let reader = CacheReader {
        root,
        index: &index,
    };
    let mut detail = reader.read_range(range)?;
    if request.before.is_some() {
        reader.expand_earlier(&mut detail, end)?;
    }
    let fresh = index.revision == request.expected_revision;
    detail.history_stale = !fresh;
    // 旧版本缓存也不再把整段账单交给后续流程。
    detail.backend_consumption = None;
    Some((detail, fresh))
}

impl CacheReader<'_> {
    fn read_page(&self, page: &PageMeta) -> Option<DbConversationDetail> {
        let file = self
            .root
            .join(&self.index.directory)
            .join(format!("{}.json", page.start));
        let mut detail: DbConversationDetail =
            serde_json::from_slice::<CachedPage>(&fs::read(file).ok()?)
                .ok()?
                .into();
        if detail.history_start != page.start
            || detail.turns.len() != page.end.checked_sub(page.start)?
        {
            return None;
        }
        detail.backend_consumption = None;
        Some(detail)
    }

    fn read_range(&self, range: Range<usize>) -> Option<DbConversationDetail> {
        if range.is_empty() {
            let mut detail = self.read_page(self.index.pages.first()?)?;
            detail.turns.clear();
            detail.history_start = range.start;
            detail.history_assistant_turns_before = 0;
            return Some(detail);
        }
        let mut result: Option<DbConversationDetail> = None;
        for page in self
            .index
            .pages
            .iter()
            .filter(|page| page.start < range.end && page.end > range.start)
        {
            let mut detail = self.read_page(page)?;
            let from = range.start.saturating_sub(page.start);
            let to = range.end.min(page.end) - page.start;
            detail.history_assistant_turns_before += count_assistants(&detail.turns[..from]);
            detail.turns = detail.turns.drain(from..to).collect();
            detail.history_start = range.start;
            if let Some(result) = result.as_mut() {
                result.turns.extend(detail.turns);
            } else {
                result = Some(detail);
            }
        }
        result.filter(|detail| detail.turns.len() == range.len())
    }

    fn expand_earlier(&self, detail: &mut DbConversationDetail, end: usize) -> Option<()> {
        let limit = end.saturating_sub(MAX_HISTORY_PAGE_TURNS);
        loop {
            if let Some(offset) = detail
                .turns
                .iter()
                .position(|turn| matches!(turn.role, TurnRole::User))
            {
                detail.history_assistant_turns_before += count_assistants(&detail.turns[..offset]);
                detail.turns.drain(..offset);
                detail.history_start += offset;
                return Some(());
            }
            let end = detail.history_start;
            if end <= limit {
                return Some(());
            }
            let mut previous =
                self.read_range(end.saturating_sub(HISTORY_PAGE_TURNS).max(limit)..end)?;
            // 只补到最近的前一条输入，避免多带另一个不完整的回合。
            let offset = previous
                .turns
                .iter()
                .rposition(|turn| matches!(turn.role, TurnRole::User))
                .unwrap_or(0);
            previous.history_assistant_turns_before += count_assistants(&previous.turns[..offset]);
            previous.turns.drain(..offset);
            previous.history_start += offset;
            detail.history_start = previous.history_start;
            detail.history_assistant_turns_before = previous.history_assistant_turns_before;
            previous.turns.append(&mut detail.turns);
            detail.turns = previous.turns;
        }
    }
}
