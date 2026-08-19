use std::path::Path;
use std::time::Instant;

use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::super::{UserMemoryRecallRequest, UserMemoryRecallScope, UserMemoryService};

pub struct PublicRecallBench {
    service: UserMemoryService,
}

pub struct PublicRecallSample {
    pub latency_us: u64,
    pub item_ids: Vec<String>,
    pub index_generation: Option<i64>,
    pub status: String,
    pub abstained: bool,
    pub reason_codes: Vec<String>,
}

impl PublicRecallBench {
    pub async fn create(db_root: &Path, source_root: &Path) -> Result<Self, String> {
        let database = crate::db::init_database_with_user_memory_root(
            db_root,
            env!("CARGO_PKG_VERSION"),
            Some(source_root),
        )
        .await
        .map_err(|error| error.to_string())?;
        let service = UserMemoryService::new(database.conn, source_root.to_path_buf());
        service
            .refresh_index()
            .await
            .map_err(|error| error.to_string())?;
        Ok(Self { service })
    }

    pub async fn recall(&self, query: &str) -> Result<PublicRecallSample, String> {
        let started = Instant::now();
        let result = self
            .service
            .recall(
                UserMemoryRecallRequest {
                    query: query.to_string(),
                    limit: Some(6),
                },
                UserMemoryRecallScope::global(),
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(PublicRecallSample {
            latency_us: started.elapsed().as_micros().min(u64::MAX as u128) as u64,
            item_ids: result.items.into_iter().map(|item| item.id).collect(),
            index_generation: result.index_generation,
            status: result.status,
            abstained: result.abstained,
            reason_codes: result.reason_codes,
        })
    }

    pub async fn indexed_item_count(&self) -> Result<usize, String> {
        let row = self
            .service
            .db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS row_count FROM memory_item_current".to_string(),
            ))
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "production index returned no item count".to_string())?;
        let count: i64 = row
            .try_get("", "row_count")
            .map_err(|error| error.to_string())?;
        usize::try_from(count).map_err(|_| format!("invalid production index item count: {count}"))
    }

    pub async fn close(self) -> Result<(), String> {
        self.service
            .db
            .close()
            .await
            .map_err(|error| error.to_string())
    }
}
