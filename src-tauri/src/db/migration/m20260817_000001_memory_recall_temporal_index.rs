use sea_orm_migration::prelude::*;

const INDEX_NAME: &str = "idx_memory_evidence_time";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_memory_evidence_time \
                 ON memory_evidence (observed_at, memory_id, id)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(&format!("DROP INDEX IF EXISTS {INDEX_NAME}"))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
    use sea_orm_migration::{MigrationTrait, SchemaManager};

    use super::Migration;

    #[tokio::test]
    async fn upgrades_existing_evidence_table_with_time_index() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared(
            "CREATE TABLE memory_evidence (id INTEGER PRIMARY KEY, memory_id TEXT NOT NULL, observed_at TEXT NOT NULL)",
        )
        .await
        .unwrap();

        Migration.up(&SchemaManager::new(&db)).await.unwrap();

        let row = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT 1 AS present FROM sqlite_master WHERE type = 'index' AND name = 'idx_memory_evidence_time'",
            ))
            .await
            .unwrap();
        assert!(row.is_some());
    }
}
