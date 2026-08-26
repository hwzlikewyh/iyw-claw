use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{DatabaseTransaction, TransactionTrait};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        let mut column_statements = Vec::new();
        for (column, statement) in INSTALLATION_COLUMNS {
            if !manager.has_column("plugin_installation", column).await? {
                column_statements.push(*statement);
            }
        }
        let add_component_config = !manager
            .has_column("plugin_component_ownership", "component_config_json")
            .await?;
        let transaction = connection.begin().await?;
        let result = async {
            for statement in column_statements {
                transaction.execute_unprepared(statement).await?;
            }
            if add_component_config {
                transaction
                    .execute_unprepared(COMPONENT_CONFIG_COLUMN)
                    .await?;
            }
            for statement in CREATE_STATE_TABLES {
                transaction.execute_unprepared(statement).await?;
            }
            transaction
                .execute_unprepared(
                    "UPDATE plugin_installation SET schema_version = 1, trust_state = 'legacy', \
                     reconcile_state = 'ready', status = 'installed' WHERE schema_version = 0",
                )
                .await?;
            Ok::<(), DbErr>(())
        }
        .await;
        finish_transaction(transaction, result).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        for table in [
            "plugin_app_instance",
            "plugin_permission_grant",
            "plugin_activation_policy",
        ] {
            connection
                .execute_unprepared(&format!("DROP TABLE IF EXISTS {table}"))
                .await?;
        }
        Ok(())
    }
}

async fn finish_transaction(
    transaction: DatabaseTransaction,
    result: Result<(), DbErr>,
) -> Result<(), DbErr> {
    match result {
        Ok(()) => transaction.commit().await,
        Err(error) => {
            let _ = transaction.rollback().await;
            Err(error)
        }
    }
}

const INSTALLATION_COLUMNS: &[(&str, &str)] = &[
    ("schema_version", "ALTER TABLE plugin_installation ADD COLUMN schema_version INTEGER NOT NULL DEFAULT 0"),
    ("publisher_id", "ALTER TABLE plugin_installation ADD COLUMN publisher_id TEXT NOT NULL DEFAULT ''"),
    ("trust_state", "ALTER TABLE plugin_installation ADD COLUMN trust_state TEXT NOT NULL DEFAULT 'untrusted'"),
    ("artifact_signature_key_id", "ALTER TABLE plugin_installation ADD COLUMN artifact_signature_key_id TEXT NOT NULL DEFAULT ''"),
    ("permissions_digest", "ALTER TABLE plugin_installation ADD COLUMN permissions_digest TEXT NOT NULL DEFAULT ''"),
    ("reconcile_state", "ALTER TABLE plugin_installation ADD COLUMN reconcile_state TEXT NOT NULL DEFAULT 'pending'"),
];

const COMPONENT_CONFIG_COLUMN: &str =
    "ALTER TABLE plugin_component_ownership ADD COLUMN component_config_json TEXT NOT NULL DEFAULT ''";

const CREATE_STATE_TABLES: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS plugin_activation_policy (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        plugin_slug TEXT NOT NULL,
        component_key TEXT NOT NULL,
        scope TEXT NOT NULL,
        workspace_key TEXT NOT NULL DEFAULT '',
        agent_type TEXT NOT NULL DEFAULT '',
        requested_enabled INTEGER NOT NULL DEFAULT 0,
        routing_mode TEXT NOT NULL,
        policy_source TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        UNIQUE(plugin_slug, component_key, scope, workspace_key, agent_type)
    )"#,
    r#"CREATE TABLE IF NOT EXISTS plugin_permission_grant (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        plugin_slug TEXT NOT NULL,
        scope TEXT NOT NULL,
        workspace_key TEXT NOT NULL DEFAULT '',
        permissions_digest TEXT NOT NULL,
        granted_permissions_json TEXT NOT NULL,
        grant_state TEXT NOT NULL,
        granted_at TEXT NULL,
        updated_at TEXT NOT NULL,
        UNIQUE(plugin_slug, scope, workspace_key)
    )"#,
    r#"CREATE TABLE IF NOT EXISTS plugin_app_instance (
        instance_id TEXT PRIMARY KEY NOT NULL,
        conversation_id INTEGER NOT NULL,
        tool_call_id TEXT NOT NULL,
        plugin_slug TEXT NOT NULL,
        plugin_version TEXT NOT NULL,
        app_key TEXT NOT NULL,
        workspace_key TEXT NOT NULL,
        launch_payload_json TEXT NOT NULL,
        state TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )"#,
    "CREATE INDEX IF NOT EXISTS idx_plugin_app_conversation ON plugin_app_instance(conversation_id, created_at)",
];
