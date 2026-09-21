use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in TABLES {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Memory authority contains durable history; automatic downgrade is unsupported".into(),
        ))
    }
}

const TABLES: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS memory_authority (root_key TEXT PRIMARY KEY, store_id TEXT NOT NULL UNIQUE, mode TEXT NOT NULL CHECK(mode IN ('shadow','active')), epoch INTEGER NOT NULL, snapshot_json TEXT NOT NULL, source_digest TEXT NOT NULL, backup_path TEXT NOT NULL, updated_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS memory_record (root_key TEXT NOT NULL, record_id TEXT NOT NULL, source_id TEXT NOT NULL, revision INTEGER NOT NULL, kind TEXT NOT NULL, content_digest TEXT NOT NULL, state TEXT NOT NULL, PRIMARY KEY(root_key,record_id), FOREIGN KEY(root_key) REFERENCES memory_authority(root_key))",
    "CREATE TABLE IF NOT EXISTS memory_revision (root_key TEXT NOT NULL, record_id TEXT NOT NULL, revision INTEGER NOT NULL, source_id TEXT NOT NULL, content_digest TEXT NOT NULL, state TEXT NOT NULL, item_json TEXT NOT NULL, reason TEXT NOT NULL, recorded_at TEXT NOT NULL, PRIMARY KEY(root_key,record_id,revision), FOREIGN KEY(root_key,record_id) REFERENCES memory_record(root_key,record_id))",
    "CREATE TABLE IF NOT EXISTS memory_identity (root_key TEXT NOT NULL, source_id TEXT NOT NULL, record_id TEXT NOT NULL, PRIMARY KEY(root_key,source_id), FOREIGN KEY(root_key,record_id) REFERENCES memory_record(root_key,record_id))",
    "CREATE TABLE IF NOT EXISTS memory_projection_job (root_key TEXT NOT NULL, epoch INTEGER NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('files','fts','vector')), state TEXT NOT NULL CHECK(state IN ('pending','done','superseded')), updated_at TEXT NOT NULL, PRIMARY KEY(root_key,epoch,kind), FOREIGN KEY(root_key) REFERENCES memory_authority(root_key))",
    "CREATE TABLE IF NOT EXISTS memory_recall_receipt (id INTEGER PRIMARY KEY AUTOINCREMENT, root_key TEXT NOT NULL, conversation_id TEXT, turn_nonce INTEGER, items_json TEXT NOT NULL, recorded_at TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS idx_memory_revision_source ON memory_revision(root_key,source_id,revision DESC)",
    "CREATE INDEX IF NOT EXISTS idx_memory_projection_pending ON memory_projection_job(root_key,state,epoch)",
    "CREATE INDEX IF NOT EXISTS idx_memory_receipt_conversation ON memory_recall_receipt(root_key,conversation_id,id DESC)",
];
