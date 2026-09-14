use std::path::Path;
use std::time::{Duration, Instant};

use sea_orm::sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use sea_orm::sqlx::{self, ConnectOptions, Connection};

use super::agent_retention::RETENTION_DAYS;
use super::agent_retention_policy::AgentLogTarget;
use super::agent_retention_scan;

const SCAN_BUDGET: Duration = Duration::from_secs(2);
const DATABASE_BUDGET: Duration = Duration::from_secs(1);
const BUSY_TIMEOUT: Duration = Duration::from_millis(100);
const BATCH_PAUSE: Duration = Duration::from_millis(25);
const DELETE_BATCH_ROWS: i64 = 200;
const MAX_DELETE_ROWS: u64 = 2_000;
const VACUUM_BATCH_PAGES: i64 = 64;
const MAX_VACUUM_PAGES: i64 = 1_024;
const INCREMENTAL_AUTO_VACUUM: i64 = 2;

pub(super) async fn cleanup(targets: Vec<AgentLogTarget>) {
    let scan = tokio::task::spawn_blocking(move || {
        agent_retention_scan::collect_groups(targets, Instant::now() + SCAN_BUDGET)
    })
    .await;
    let scan = match scan {
        Ok(scan) => scan,
        Err(error) => {
            tracing::warn!(error = %error, "[logs] Codex database scan failed");
            return;
        }
    };
    if scan.failed_files > 0 || scan.timed_out {
        tracing::warn!(
            error = scan
                .first_error
                .as_deref()
                .unwrap_or("scan budget exceeded"),
            "[logs] Codex database scan incomplete"
        );
    }
    for group in scan.groups {
        for file in group.files.into_iter().filter(|file| file.primary) {
            if let Err(error) = cleanup_database(&file.path).await {
                tracing::warn!(
                    database = file.path.file_name().and_then(|name| name.to_str()),
                    error = %error,
                    "[logs] Codex database cleanup deferred until next retention run"
                );
            }
        }
    }
}

async fn cleanup_database(path: &Path) -> Result<(), sqlx::Error> {
    let started = Instant::now();
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .busy_timeout(BUSY_TIMEOUT)
        .disable_statement_logging();
    let mut connection = SqliteConnection::connect_with(&options).await?;
    let result = maintain(&mut connection, started + DATABASE_BUDGET).await;
    let close_result = connection.close().await;
    let (deleted_rows, reclaimed_pages) = result?;
    close_result?;
    if deleted_rows > 0 || reclaimed_pages > 0 {
        tracing::info!(
            database = path.file_name().and_then(|name| name.to_str()),
            deleted_rows,
            reclaimed_pages,
            retention_days = RETENTION_DAYS,
            elapsed_ms = started.elapsed().as_millis(),
            "[logs] Codex database retention completed"
        );
    }
    Ok(())
}

async fn maintain(
    connection: &mut SqliteConnection,
    deadline: Instant,
) -> Result<(u64, i64), sqlx::Error> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(RETENTION_DAYS as i64)).timestamp();
    let mut deleted_rows = 0;
    while Instant::now() < deadline && deleted_rows < MAX_DELETE_ROWS {
        let result = sqlx::query(
            "DELETE FROM logs WHERE id IN (SELECT id FROM logs WHERE ts < ? ORDER BY ts LIMIT ?)",
        )
        .bind(cutoff)
        .bind(DELETE_BATCH_ROWS)
        .execute(&mut *connection)
        .await?;
        deleted_rows += result.rows_affected();
        if result.rows_affected() < DELETE_BATCH_ROWS as u64 {
            break;
        }
        tokio::time::sleep(BATCH_PAUSE).await;
    }
    let reclaimed_pages = reclaim_pages(connection, deadline).await?;
    // PASSIVE 不等待其他读写连接，也不删除仍在使用的 WAL 文件。
    sqlx::query("PRAGMA wal_checkpoint(PASSIVE)")
        .fetch_all(&mut *connection)
        .await?;
    Ok((deleted_rows, reclaimed_pages))
}

async fn reclaim_pages(
    connection: &mut SqliteConnection,
    deadline: Instant,
) -> Result<i64, sqlx::Error> {
    let mode: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(&mut *connection)
        .await?;
    if mode != INCREMENTAL_AUTO_VACUUM {
        return Ok(0);
    }
    let mut reclaimed = 0;
    while Instant::now() < deadline && reclaimed < MAX_VACUUM_PAGES {
        let before: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(&mut *connection)
            .await?;
        if before == 0 {
            break;
        }
        // PRAGMA 不支持绑定参数；这里只使用固定的内部批量上限。
        sqlx::query(&format!("PRAGMA incremental_vacuum({VACUUM_BATCH_PAGES})"))
            .fetch_all(&mut *connection)
            .await?;
        let after: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(&mut *connection)
            .await?;
        if after >= before {
            break;
        }
        reclaimed += before - after;
        tokio::time::sleep(BATCH_PAUSE).await;
    }
    Ok(reclaimed)
}
