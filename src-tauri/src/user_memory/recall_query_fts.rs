use std::collections::BTreeMap;

use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement};

use super::recall_query::{LaneCollection, MAX_LANE_CANDIDATES};
use super::recall_rank::{add_rows, Candidate, LaneScore};
use super::recall_scope::UserMemoryRecallScope;
use super::recall_validity::{push_query_at, valid_at_sql};

const FTS_SCAN_LIMIT: usize = MAX_LANE_CANDIDATES * 4;
const MAX_CJK_TRIGRAMS: usize = 24;
/// 内嵌英文词的最短长度，更短的片段噪声明显。
const MIN_EMBEDDED_WORD_CHARS: usize = 3;
const MAX_EMBEDDED_WORDS: usize = 12;
/// 超过该长度的查询多为粘贴的日志或文档，抽词只会引入噪声。
const MAX_EMBEDDED_WORD_QUERY_CHARS: usize = 128;
const EMBEDDED_WORD_STOPLIST: [&str; 4] = ["http", "https", "www", "com"];

pub(super) struct FtsQuery<'a> {
    pub query: &'a str,
    pub query_at: &'a str,
    pub table: &'static str,
    pub weight: f64,
    pub scope: &'a UserMemoryRecallScope,
}

struct FtsMatchQueries {
    primary: String,
    fallback: Option<String>,
}

pub(super) async fn collect_fts<C: ConnectionTrait>(
    db: &C,
    query: FtsQuery<'_>,
    out: &mut BTreeMap<String, Candidate>,
) -> Result<LaneCollection, String> {
    let lane = fts_lane_name(query.table);
    let minimum_token_chars = if query.table.ends_with("_trigram") {
        3
    } else {
        1
    };
    let Some(match_queries) = fts_queries(query.query, minimum_token_chars) else {
        return Ok(LaneCollection::skipped("query_has_no_tokens"));
    };
    let mut rows = collect_fts_variant(db, &query, &match_queries.primary, lane, true).await?;
    if rows.is_empty() {
        if let Some(fallback) = match_queries.fallback.as_deref() {
            rows = collect_fts_variant(db, &query, fallback, lane, false).await?;
        }
    }
    let candidate_count = rows.len();
    add_rows(
        rows,
        out,
        LaneScore {
            name: lane,
            weight: query.weight,
        },
    );
    Ok(LaneCollection::collected(candidate_count))
}

async fn collect_fts_variant<C: ConnectionTrait>(
    db: &C,
    query: &FtsQuery<'_>,
    match_query: &str,
    lane: &str,
    include_candidates: bool,
) -> Result<Vec<QueryResult>, String> {
    let rows = execute_fts_query(
        db,
        fast_fts_statement(query, match_query, include_candidates),
        query.table,
        lane,
    )
    .await?;
    if rows.len() >= MAX_LANE_CANDIDATES
        || !has_fts_match(db, query.table, match_query, lane).await?
    {
        return Ok(rows);
    }
    // 预取页不足时必须回退到过滤后截断，避免不可见高排名项遮挡后续合法项。
    execute_fts_query(
        db,
        filtered_fts_statement(query, match_query, include_candidates),
        query.table,
        lane,
    )
    .await
}

fn fast_fts_statement(
    query: &FtsQuery<'_>,
    match_query: &str,
    include_candidates: bool,
) -> Statement {
    let validity = valid_at_sql("memory_item_current");
    let scope = query.scope.predicate("memory_item_current");
    let trust = trust_predicate(include_candidates);
    let sql = format!(
        "SELECT memory_item_current.id FROM (SELECT {table}.rowid, rank AS fts_rank FROM {table} WHERE {table} MATCH ? LIMIT ?) AS matches CROSS JOIN memory_item_current ON memory_item_current.row_id = matches.rowid WHERE {scope} AND {trust} AND memory_item_current.sensitive = 0 AND memory_item_current.superseded_by IS NULL{validity} ORDER BY matches.fts_rank, memory_item_current.id LIMIT ?",
        table = query.table,
    );
    let mut values = vec![
        match_query.to_string().into(),
        (FTS_SCAN_LIMIT as i64).into(),
    ];
    query.scope.push_bind(&mut values);
    push_query_at(&mut values, query.query_at);
    values.push((MAX_LANE_CANDIDATES as i64).into());
    Statement::from_sql_and_values(DbBackend::Sqlite, sql, values)
}

async fn has_fts_match<C: ConnectionTrait>(
    db: &C,
    table: &str,
    match_query: &str,
    lane: &str,
) -> Result<bool, String> {
    let sql = format!("SELECT rowid FROM {table} WHERE {table} MATCH ? LIMIT 1");
    let statement =
        Statement::from_sql_and_values(DbBackend::Sqlite, sql, [match_query.to_string().into()]);
    execute_fts_query(db, statement, table, lane)
        .await
        .map(|rows| !rows.is_empty())
}

fn filtered_fts_statement(
    query: &FtsQuery<'_>,
    match_query: &str,
    include_candidates: bool,
) -> Statement {
    let validity = valid_at_sql("memory_item_current");
    let scope = query.scope.predicate("memory_item_current");
    let trust = trust_predicate(include_candidates);
    let sql = format!(
        "SELECT memory_item_current.id FROM (SELECT {table}.rowid, rank AS fts_rank FROM {table} WHERE {table} MATCH ? AND EXISTS (SELECT 1 FROM memory_item_current WHERE memory_item_current.row_id = {table}.rowid AND {scope} AND {trust} AND memory_item_current.sensitive = 0 AND memory_item_current.superseded_by IS NULL{validity}) ORDER BY rank LIMIT ?) AS matches CROSS JOIN memory_item_current ON memory_item_current.row_id = matches.rowid ORDER BY matches.fts_rank, memory_item_current.id",
        table = query.table,
    );
    let mut values = vec![match_query.to_string().into()];
    query.scope.push_bind(&mut values);
    push_query_at(&mut values, query.query_at);
    values.push((MAX_LANE_CANDIDATES as i64).into());
    Statement::from_sql_and_values(DbBackend::Sqlite, sql, values)
}

fn trust_predicate(include_candidates: bool) -> &'static str {
    if include_candidates {
        "memory_item_current.trust_class IN ('host_confirmed','agent_experience','candidate')"
    } else {
        "memory_item_current.trust_class IN ('host_confirmed','agent_experience')"
    }
}

async fn execute_fts_query<C: ConnectionTrait>(
    db: &C,
    statement: Statement,
    table: &str,
    lane: &str,
) -> Result<Vec<QueryResult>, String> {
    db.query_all_raw(statement).await.map_err(|error| {
        tracing::debug!(table, error = %error, "[memory-recall] FTS lane unavailable");
        format!("fts_{lane}_error")
    })
}

fn fts_lane_name(table: &'static str) -> &'static str {
    table.strip_prefix("memory_item_fts_").unwrap_or(table)
}

fn fts_queries(query: &str, minimum_token_chars: usize) -> Option<FtsMatchQueries> {
    let raw_tokens = query
        .split_whitespace()
        .filter(|token| token.chars().count() >= minimum_token_chars)
        .collect::<Vec<_>>();
    if raw_tokens.is_empty() {
        return None;
    }
    let mut variants = raw_tokens
        .iter()
        .map(|token| quote_fts_term(token))
        .collect::<Vec<_>>();
    let primary = variants.join(" ");
    let embedded_words = embedded_ascii_words(query, minimum_token_chars);
    variants.extend(
        embedded_words
            .iter()
            .filter(|word| {
                !raw_tokens
                    .iter()
                    .any(|token| token.eq_ignore_ascii_case(word))
            })
            .map(|word| quote_fts_term(word)),
    );
    if minimum_token_chars == 3 {
        variants.extend(cjk_trigrams(query).iter().map(|gram| quote_fts_term(gram)));
    }
    // 仅有单个原始 token 且没有扩展词时，fallback 与 primary 等价，无需重复查询。
    let fallback = (variants.len() > 1).then(|| variants.join(" OR "));
    Some(FtsMatchQueries { primary, fallback })
}

fn quote_fts_term(term: &str) -> String {
    format!("\"{}\"", term.replace('"', " "))
}

/// 抽取中文 token 内嵌的英文词：unicode61 会把相连的中英文当成一个 token，
/// 例如“用dbx去操作”中的 dbx 无法单独匹配，需要拆出来参与 fallback。
fn embedded_ascii_words(query: &str, minimum_token_chars: usize) -> Vec<String> {
    let mut words = Vec::new();
    if query.chars().count() > MAX_EMBEDDED_WORD_QUERY_CHARS {
        return words;
    }
    let minimum_chars = MIN_EMBEDDED_WORD_CHARS.max(minimum_token_chars);
    let pieces = query
        .split_whitespace()
        .filter(|token| token.chars().any(is_cjk))
        .flat_map(|token| token.split(|character: char| !is_embedded_word_char(character)));
    for piece in pieces {
        let word = piece.trim_matches(['_', '-']).to_ascii_lowercase();
        if word.len() < minimum_chars || is_noise_word(&word) || words.contains(&word) {
            continue;
        }
        words.push(word);
        if words.len() >= MAX_EMBEDDED_WORDS {
            break;
        }
    }
    words
}

fn is_embedded_word_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

fn is_noise_word(word: &str) -> bool {
    word.bytes().all(|byte| byte.is_ascii_digit()) || EMBEDDED_WORD_STOPLIST.contains(&word)
}

fn cjk_trigrams(query: &str) -> Vec<String> {
    let mut trigrams = Vec::new();
    let mut run = Vec::new();
    for character in query.chars() {
        if is_cjk(character) {
            run.push(character);
        } else {
            append_trigrams(&mut trigrams, &run);
            run.clear();
        }
        if trigrams.len() >= MAX_CJK_TRIGRAMS {
            break;
        }
    }
    if trigrams.len() < MAX_CJK_TRIGRAMS {
        append_trigrams(&mut trigrams, &run);
    }
    trigrams.truncate(MAX_CJK_TRIGRAMS);
    trigrams
}

fn append_trigrams(output: &mut Vec<String>, run: &[char]) {
    for window in run.windows(3) {
        output.push(window.iter().collect());
        if output.len() >= MAX_CJK_TRIGRAMS {
            break;
        }
    }
}

fn is_cjk(character: char) -> bool {
    matches!(character, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}')
}
