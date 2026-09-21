use super::authority_types::AuthorityData;
use super::{authority_sql as sql, structured_file, UserMemoryService};
use crate::app_error::AppCommandError;

impl UserMemoryService {
    pub(super) async fn backup_authority_source(
        &self,
        data: &AuthorityData,
    ) -> Result<String, AppCommandError> {
        let prefix = format!(".memory-authority-backup-{}", uuid::Uuid::new_v4().simple());
        let root = self.resolved_root()?;
        let source_name = format!("{prefix}.json");
        let database_name = format!("{prefix}.db");
        structured_file::install_new_private(
            root,
            &source_name,
            super::authority::encode(data)?.as_bytes(),
        )?;
        // SQLite允许空的目标文件；先以私有权限独占创建，再写入一致性快照。
        structured_file::install_new_private(root, &database_name, b"")?;
        let database = root.join(&database_name);
        sql::execute(
            &self.db,
            "VACUUM INTO ?",
            vec![database.to_string_lossy().into_owned().into()],
        )
        .await?;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&database)
            .and_then(|file| file.sync_all())
            .map_err(AppCommandError::io)?;
        verify_database(&database).await?;
        let restored: AuthorityData =
            structured_file::read_json_optional(root, &source_name, BACKUP_LIMIT)?.ok_or_else(
                || AppCommandError::configuration_invalid("Memory source backup is missing"),
            )?;
        if &restored != data {
            return Err(AppCommandError::configuration_invalid(
                "Memory source backup verification failed",
            ));
        }
        Ok(root.join(source_name).to_string_lossy().into_owned())
    }
}

const BACKUP_LIMIT: usize = 20_971_520;

async fn verify_database(path: &std::path::Path) -> Result<(), AppCommandError> {
    let url = format!(
        "sqlite:{}?mode=ro",
        path.to_string_lossy().replace('\\', "/")
    );
    let backup = sea_orm::Database::connect(url)
        .await
        .map_err(super::index_checkpoint::database_error)?;
    let rows = sql::rows(&backup, "PRAGMA quick_check", vec![]).await?;
    let valid = rows.len() == 1 && sql::field::<String>(&rows[0], "quick_check")? == "ok";
    backup
        .close()
        .await
        .map_err(super::index_checkpoint::database_error)?;
    if !valid {
        return Err(AppCommandError::configuration_invalid(
            "Memory database backup integrity check failed",
        ));
    }
    Ok(())
}
