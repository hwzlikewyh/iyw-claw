//! 激活辅助：健康失败时的隔离、LKG 回滚与活跃会话的延迟激活标记。
//!
//! 激活原则：先写 inventory generation，再原子切换 active pointer；健康检查失败
//! 恢复旧 active / LKG，保留失败诊断并把新版本移入 quarantine，绝不把未验证版本
//! 留在 active 位置。

use std::path::{Path, PathBuf};

use crate::acp::version_center::installer::runtime::restore_current_pointer;
use crate::app_error::AppCommandError;

/// quarantine 根目录：可清理临时区，不参与 active 解析。
pub fn quarantine_root(data_dir: &Path) -> PathBuf {
    data_dir.join("staging").join("quarantine")
}

/// 把 `<root>/<kind>/<id>/<version>` 移入 quarantine，返回隔离后路径。
///
/// 安全要求：源与目标都必须先在受管根下 canonicalize，防止路径逃逸。
pub async fn quarantine_component(
    data_dir: &Path,
    component_dir: &Path,
) -> Result<PathBuf, AppCommandError> {
    let managed_root = data_dir.canonicalize().map_err(|error| {
        AppCommandError::configuration_invalid("Managed root is not readable")
            .with_detail(error.to_string())
    })?;
    let source = component_dir.canonicalize().map_err(|error| {
        AppCommandError::invalid_input("Component directory is not readable")
            .with_detail(error.to_string())
    })?;
    if !source.starts_with(&managed_root) {
        return Err(AppCommandError::invalid_input(
            "Component directory is outside the managed root",
        ));
    }
    let target_root = quarantine_root(data_dir);
    std::fs::create_dir_all(&target_root).map_err(AppCommandError::io)?;
    let name = source
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "component".to_string());
    let target = target_root.join(format!(
        "{name}-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("0")
    ));
    let absolute_target = target.canonicalize().unwrap_or(target.clone());
    if !absolute_target.starts_with(target_root) {
        return Err(AppCommandError::invalid_input(
            "Quarantine target is outside the quarantine root",
        ));
    }
    tokio::fs::rename(&source, &target)
        .await
        .map_err(|error| AppCommandError::io(error).with_detail(source.to_string_lossy().into_owned()))?;
    Ok(target)
}

/// 恢复旧 active pointer（LKG）。`previous` 为 `None` 时删除 pointer。
pub async fn restore_lkg_pointer(
    data_dir: &Path,
    tool_id: &str,
    previous: Option<Vec<u8>>,
) -> Result<(), AppCommandError> {
    restore_current_pointer(data_dir, tool_id, previous).await
}

/// 组件目录是否可安全进入 quarantine（目录存在且位于受管根下）。
pub async fn quarantine_candidate(data_dir: &Path, component_dir: &Path) -> bool {
    if !tokio::fs::metadata(component_dir)
        .await
        .is_ok_and(|metadata| metadata.is_dir())
    {
        return false;
    }
    let Ok(managed_root) = data_dir.canonicalize() else {
        return false;
    };
    let Ok(source) = component_dir.canonicalize() else {
        return false;
    };
    source.starts_with(&managed_root)
}
