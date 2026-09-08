//! 激活辅助：健康失败时的隔离、LKG 回滚与活跃会话的延迟激活标记。
//!
//! 激活原则：先写 inventory generation，再原子切换 active pointer；健康检查失败
//! 恢复旧 active / LKG，保留失败诊断并把新版本移入 quarantine，绝不把未验证版本
//! 留在 active 位置。

use std::path::{Path, PathBuf};

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
    let source = component_dir.canonicalize().map_err(|error| {
        AppCommandError::invalid_input("Component directory is not readable")
            .with_detail(error.to_string())
    })?;
    let target_root = component_quarantine_root(data_dir, &source)?;
    let parent = target_root
        .parent()
        .ok_or_else(|| AppCommandError::invalid_input("Quarantine root has no parent"))?
        .to_path_buf();
    std::fs::create_dir_all(&target_root).map_err(AppCommandError::io)?;
    let target_root = target_root.canonicalize().map_err(AppCommandError::io)?;
    if !target_root.starts_with(parent.canonicalize().map_err(AppCommandError::io)?) {
        return Err(AppCommandError::invalid_input(
            "Quarantine root escapes its parent",
        ));
    }
    let name = source
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "component".to_string());
    let target = target_root.join(format!(
        "{name}-{}",
        uuid::Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("0")
    ));
    tokio::fs::rename(&source, &target).await.map_err(|error| {
        AppCommandError::io(error).with_detail(source.to_string_lossy().into_owned())
    })?;
    Ok(target)
}

fn component_quarantine_root(data_dir: &Path, source: &Path) -> Result<PathBuf, AppCommandError> {
    for tool in crate::shared_runtime::SHARED_TOOLS {
        let root = crate::shared_runtime::tool_root(data_dir, tool);
        if let Ok(canonical) = root.canonicalize() {
            if source.starts_with(&canonical) && source != canonical {
                return Ok(canonical.join(".quarantine"));
            }
        }
    }
    let root = data_dir.canonicalize().map_err(AppCommandError::io)?;
    if source.starts_with(&root) && source != root {
        return Ok(quarantine_root(data_dir));
    }
    Err(AppCommandError::invalid_input(
        "Component directory is outside the managed root",
    ))
}
