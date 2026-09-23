use std::path::Path;

use crate::app_error::AppCommandError;
use crate::parsers::ExternalSource;

pub(super) async fn snapshot_sources(work: &Path) -> Result<Vec<ExternalSource>, AppCommandError> {
    let mut sources = super::external::sources();
    for source in &mut sources {
        if !source.is_file
            || !source.root.is_file()
            || source.root.extension().is_none_or(|ext| ext != "db")
        {
            continue;
        }
        let destination = work.join(source.agent).join(
            source
                .root
                .file_name()
                .ok_or_else(super::unknown_format_error)?,
        );
        tokio::fs::create_dir_all(
            destination
                .parent()
                .ok_or_else(super::unknown_format_error)?,
        )
        .await
        .map_err(AppCommandError::io)?;
        let connection = super::portable_database::open(&source.root, true).await?;
        // SQLite 在线快照包含已提交的 WAL，不能直接复制主文件。
        let result = super::core::snapshot_db_to(&connection, &destination).await;
        let closed = connection.close().await.map_err(|error| {
            AppCommandError::database_error("Close transcript snapshot source")
                .with_detail(error.to_string())
        });
        result?;
        closed?;
        source.root = destination;
        tracing::info!(
            agent = source.agent,
            "[BACKUP] native session database snapshotted"
        );
    }
    Ok(sources)
}
