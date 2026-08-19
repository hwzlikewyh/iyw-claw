use sea_orm_migration::prelude::*;

const TABLES: [&str; 5] = [
    "memory_item_current",
    "memory_alias_current",
    "memory_evidence",
    "memory_relation_current",
    "memory_source_checkpoint",
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        create_memory_tables(db).await?;
        create_memory_indexes(db).await?;
        create_optional_fts_tables(db).await;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS memory_item_fts_trigram")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS memory_item_fts_unicode")
            .await?;
        for table in TABLES.iter().rev() {
            db.execute_unprepared(&format!("DROP TABLE IF EXISTS {table}"))
                .await?;
        }
        Ok(())
    }
}

async fn create_memory_tables(db: &SchemaManagerConnection<'_>) -> Result<(), DbErr> {
    create_item_table(db).await?;
    create_alias_table(db).await?;
    create_evidence_relation_checkpoint_tables(db).await
}

async fn create_item_table(db: &SchemaManagerConnection<'_>) -> Result<(), DbErr> {
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS memory_item_current (\
                row_id INTEGER PRIMARY KEY AUTOINCREMENT,\
                id TEXT NOT NULL UNIQUE,\
                kind TEXT NOT NULL,\
                trust_class TEXT NOT NULL,\
                scope_type TEXT NOT NULL,\
                scope_key TEXT NOT NULL,\
                content TEXT NOT NULL,\
                content_digest TEXT NOT NULL,\
                confidence INTEGER NOT NULL DEFAULT 100,\
                importance REAL NOT NULL DEFAULT 0,\
                valid_from TEXT,\
                valid_to TEXT,\
                source_revision TEXT NOT NULL,\
                sensitive INTEGER NOT NULL DEFAULT 0,\
                superseded_by TEXT\
            )",
    )
    .await?;
    Ok(())
}

async fn create_alias_table(db: &SchemaManagerConnection<'_>) -> Result<(), DbErr> {
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS memory_alias_current (\
                memory_id TEXT NOT NULL,\
                alias_kind TEXT NOT NULL,\
                alias TEXT NOT NULL,\
                normalized_alias TEXT NOT NULL,\
                scope_type TEXT NOT NULL,\
                scope_key TEXT NOT NULL,\
                FOREIGN KEY (memory_id) REFERENCES memory_item_current(id) ON DELETE CASCADE,\
                UNIQUE (normalized_alias, scope_type, scope_key, memory_id)\
            )",
    )
    .await?;
    Ok(())
}

async fn create_evidence_relation_checkpoint_tables(
    db: &SchemaManagerConnection<'_>,
) -> Result<(), DbErr> {
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS memory_evidence (\
                id INTEGER PRIMARY KEY AUTOINCREMENT,\
                memory_id TEXT NOT NULL,\
                source_kind TEXT NOT NULL,\
                source_id TEXT NOT NULL,\
                conversation_id TEXT,\
                turn_nonce INTEGER,\
                excerpt_digest TEXT NOT NULL,\
                observed_at TEXT NOT NULL,\
                FOREIGN KEY (memory_id) REFERENCES memory_item_current(id) ON DELETE CASCADE,\
                UNIQUE (memory_id, source_kind, source_id, turn_nonce)\
            )",
    )
    .await?;
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS memory_relation_current (\
                source_id TEXT NOT NULL,\
                relation TEXT NOT NULL,\
                target_id TEXT NOT NULL,\
                confidence INTEGER NOT NULL DEFAULT 100,\
                created_at TEXT NOT NULL,\
                FOREIGN KEY (source_id) REFERENCES memory_item_current(id) ON DELETE CASCADE,\
                FOREIGN KEY (target_id) REFERENCES memory_item_current(id) ON DELETE CASCADE,\
                CHECK (source_id <> target_id),\
                UNIQUE (source_id, relation, target_id)\
            )",
    )
    .await?;
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS memory_source_checkpoint (\
                source_key TEXT PRIMARY KEY,\
                source_digest TEXT NOT NULL,\
                index_generation INTEGER NOT NULL,\
                indexed_at TEXT NOT NULL,\
                status TEXT NOT NULL,\
                fts_unicode_status TEXT NOT NULL,\
                fts_trigram_status TEXT NOT NULL,\
                last_error TEXT\
            )",
    )
    .await
    .map(|_| ())
}

async fn create_memory_indexes(db: &SchemaManagerConnection<'_>) -> Result<(), DbErr> {
    for sql in [
            "CREATE INDEX IF NOT EXISTS idx_memory_item_scope_time ON memory_item_current (scope_type, scope_key, trust_class, valid_from, valid_to, row_id)",
            "CREATE INDEX IF NOT EXISTS idx_memory_alias_lookup ON memory_alias_current (normalized_alias, scope_type, scope_key, memory_id)",
            "CREATE INDEX IF NOT EXISTS idx_memory_evidence_lookup ON memory_evidence (memory_id, observed_at, id)",
            "CREATE INDEX IF NOT EXISTS idx_memory_relation_lookup ON memory_relation_current (source_id, relation, confidence, target_id)",
    ] {
        db.execute_unprepared(sql).await?;
    }
    Ok(())
}

async fn create_optional_fts_tables(db: &SchemaManagerConnection<'_>) {
    // FTS5 is optional. A missing tokenizer must not fail the migration.
    for (name, sql) in [
            (
                "unicode",
                "CREATE VIRTUAL TABLE IF NOT EXISTS memory_item_fts_unicode USING fts5(content, content='memory_item_current', content_rowid='row_id', tokenize='unicode61')",
            ),
            (
                "trigram",
                "CREATE VIRTUAL TABLE IF NOT EXISTS memory_item_fts_trigram USING fts5(content, content='memory_item_current', content_rowid='row_id', tokenize='trigram case_sensitive 0')",
            ),
    ] {
        if let Err(error) = db.execute_unprepared(sql).await {
            tracing::info!(
                lane = name,
                error = %error,
                "optional memory FTS lane is unavailable"
            );
        }
    }
}
