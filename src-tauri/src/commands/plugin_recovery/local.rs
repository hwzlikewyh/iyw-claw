use crate::app_error::AppCommandError;
use crate::db::service::plugin_installation_service::PluginInstallationRecord;
use std::path::{Path, PathBuf};

const MAX_POINTER_BYTES: u64 = 128 * 1024;

fn require_managed_ancestors(root: &Path, base: &Path) -> Result<(), AppCommandError> {
    let relative = root
        .strip_prefix(base)
        .map_err(|_| invalid("插件目录越界"))?;
    let mut current = base.to_path_buf();
    for part in relative.components() {
        current.push(part);
        let metadata =
            std::fs::symlink_metadata(&current).map_err(|_| invalid("插件目录不可用"))?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const REPARSE_POINT: u32 = 0x0000_0400;
            if metadata.file_attributes() & REPARSE_POINT != 0 {
                return Err(invalid("插件受管目录不允许重解析点"));
            }
        }
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid("插件受管目录不允许链接"));
        }
    }
    Ok(())
}

pub(crate) async fn verify(record: &PluginInstallationRecord) -> Result<PathBuf, AppCommandError> {
    let installation = record.installation.clone();
    tokio::task::spawn_blocking(move || verify_local(&installation))
        .await
        .map_err(|_| invalid("插件本地验证任务未完成"))?
}

fn managed_root(
    installation: &crate::db::entities::plugin_installation::Model,
) -> Result<PathBuf, AppCommandError> {
    let paths = crate::acp::agent_storage::AgentStoragePaths::active()
        .ok_or_else(|| invalid("插件存储未初始化"))?;
    if semver::Version::parse(&installation.version).is_err()
        || installation.slug.is_empty()
        || installation
            .slug
            .chars()
            .any(|character| !character.is_ascii_alphanumeric() && character != '-')
    {
        return Err(invalid("插件安装身份无效"));
    }
    let root = paths
        .plugins_dir()
        .join(&installation.slug)
        .join("versions")
        .join(&installation.version);
    if root != Path::new(&installation.install_root) {
        return Err(invalid("插件安装位置与受管目录不符"));
    }
    require_managed_ancestors(&root, &paths.plugins_dir())?;
    let base = paths
        .plugins_dir()
        .canonicalize()
        .map_err(|_| invalid("插件目录不可用"))?;
    let canonical = root
        .canonicalize()
        .map_err(|_| invalid("插件安装包文件缺失"))?;
    if !canonical.starts_with(base) {
        return Err(invalid("插件安装目录越界"));
    }
    Ok(root)
}

fn verify_local(
    installation: &crate::db::entities::plugin_installation::Model,
) -> Result<PathBuf, AppCommandError> {
    let root = managed_root(installation)?;
    let package =
        crate::acp::skill_package::validate_plugin_directory(&root, &installation.content_sha256)?;
    let manifest: crate::commands::skill_market::SkillPluginManifest =
        serde_json::from_str(&installation.manifest_json)
            .map_err(|_| invalid("插件本地清单记录损坏"))?;
    crate::commands::skill_market::plugin_client_version::require_current(&manifest.min_client_version)?;
    crate::commands::skill_market::validate_installed_plugin_package(
        &package,
        &manifest,
        &installation.slug,
        &installation.version,
    )?;
    Ok(root
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| invalid("插件目录结构无效"))?
        .join("current.json"))
}

pub(super) fn read_pointer(path: &Path) -> Result<Option<Vec<u8>>, AppCommandError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid("插件指针无法读取")),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_POINTER_BYTES
    {
        return Err(invalid("插件指针不是有效的受管文件"));
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_POINTER_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| invalid("插件指针无法读取"))?;
    if bytes.len() as u64 != metadata.len() {
        return Err(invalid("插件指针在读取期间发生变化"));
    }
    Ok(Some(bytes))
}

pub(super) fn write_pointer(path: &Path, bytes: &[u8]) -> Result<(), AppCommandError> {
    use std::io::Write;
    let parent = path.parent().ok_or_else(|| invalid("插件指针路径无效"))?;
    let mut file =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| invalid("插件恢复临时文件无法创建"))?;
    file.write_all(bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| invalid("插件恢复指针无法写入"))?;
    file.persist(path)
        .map_err(|_| invalid("插件恢复指针无法提交，原指针保留"))?;
    Ok(())
}

fn invalid(message: &str) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}
