use std::path::{Path, PathBuf};

use super::{image_error, managed_location, prepare_scope, reject_symlink};
use super::{AcpError, CodexImageScope, ManagedLocation, PreparedScope};

pub(crate) async fn validate_path(
    scope: CodexImageScope,
    source: &Path,
) -> Result<PathBuf, AcpError> {
    // 主进程启动时已固定该权威目录；未初始化时拒绝访问工作区外附件。
    let data_dir = std::env::var_os("IYW_CLAW_DATA_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| image_error("图片附件存储尚未初始化，请重新连接会话"))?;
    let scope = prepare_scope(&data_dir, scope).await?;
    reject_symlink(source).await?;
    let path = tokio::fs::canonicalize(source)
        .await
        .map_err(|error| super::io_error("image tool attachment", error))?;
    let location = managed_location(&scope.data_dir, &path)?;
    if !belongs_to_scope(&scope, &location) {
        return Err(image_error(
            "图片附件不属于当前会话，请使用本轮提供的附件路径",
        ));
    }
    Ok(path)
}

pub(super) fn belongs_to_scope(scope: &PreparedScope, location: &ManagedLocation) -> bool {
    match location {
        ManagedLocation::Chat(chat_dir) => scope.chat_dir.as_ref() == Some(chat_dir),
        ManagedLocation::Conversation(bucket) => scope.allowed_buckets.contains(bucket),
    }
}
