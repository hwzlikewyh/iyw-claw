//! 旧目录一次性导入：把历史包内 / 旧 cache 的运行时代码导入新库存。
//!
//! 规则：
//! - 只有通过布局校验的版本目录才导入，写入 `origin: legacy` 的 ownership
//!   marker（无 artifact ID / SHA-256，因为旧内容没有可信摘要）。
//! - 无可信摘要的旧内容只能作为 system fallback 或由后端计划重新下载，绝不伪装
//!   成 managed artifact。
//! - 用户 Skill 与 dirty checkout 不导入受管系统目录。
//! - migration receipt 可重复读取，不重复搬运。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::manifest::{
    upsert_entry, write_marker, write_manifest, InventoryEntry, InventoryManifest,
    OwnershipMarker,
};
use crate::acp::version_center::capability;
use crate::app_error::AppCommandError;

const MIGRATION_RECEIPT_FILE: &str = "migration-receipt.json";
const MIGRATION_SCHEMA_VERSION: u32 = 1;
pub const ORIGIN_LEGACY: &str = "legacy";

/// 单条迁移记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyMigrationEntry {
    pub component_id: String,
    pub component_kind: String,
    pub version: String,
    pub from: String,
    pub origin: String,
    pub verified: bool,
    #[serde(default)]
    pub note: String,
}

/// 迁移 receipt：已存在即视为迁移完成，可重复读取。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyMigrationReceipt {
    pub schema_version: u32,
    pub migrated_at: String,
    #[serde(default)]
    pub entries: Vec<LegacyMigrationEntry>,
}

/// 迁移报告（UI 展示）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyMigrationReport {
    pub migrated: usize,
    pub skipped: usize,
    pub receipt_written: bool,
}

pub fn receipt_path(data_dir: &Path) -> PathBuf {
    data_dir.join("inventory").join(MIGRATION_RECEIPT_FILE)
}

/// 读取迁移 receipt；未执行过迁移时返回 `None`。
pub async fn migration_receipt(
    data_dir: &Path,
) -> Result<Option<LegacyMigrationReceipt>, AppCommandError> {
    match tokio::fs::read_to_string(receipt_path(data_dir)).await {
        Ok(raw) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|error| {
                AppCommandError::configuration_invalid("Migration receipt is corrupted")
                    .with_detail(error.to_string())
            }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppCommandError::io(error)),
    }
}

/// 执行一次性迁移。receipt 已存在时直接返回报告（不重复搬运）。
pub async fn run_legacy_migration(
    data_dir: &Path,
) -> Result<LegacyMigrationReport, AppCommandError> {
    if let Some(receipt) = migration_receipt(data_dir).await? {
        return Ok(LegacyMigrationReport {
            migrated: receipt.entries.len(),
            skipped: 0,
            receipt_written: true,
        });
    }
    let mut entries: Vec<LegacyMigrationEntry> = Vec::new();
    let mut manifest = super::manifest::read_manifest(data_dir).await?;

    // 只扫描 runtime/<tool>/<version>/<platform> 布局（历史受管运行时的落点）。
    let runtime_root = data_dir.join("runtime");
    for tool in capability::TOOL_IDS {
        let tool_root = runtime_root.join(tool);
        let Ok(versions) = tokio::fs::read_dir(&tool_root).await else {
            continue;
        };
        let mut versions = versions;
        while let Ok(Some(version_entry)) = versions.next_entry().await {
            let version_dir = version_entry.path();
            if !version_entry
                .file_type()
                .await
                .is_ok_and(|kind| kind.is_dir())
            {
                continue;
            }
            let version = version_entry.file_name().to_string_lossy().into_owned();
            let Some(platform_dir) = legacy_platform_dir(&version_dir, tool).await else {
                continue;
            };
            let (verified, note) = verify_legacy_layout(tool, &platform_dir);
            if !verified {
                continue; // 布局不合格的旧目录不导入。
            }
            let relative = format!(
                "runtime/{tool}/{version}/{}",
                platform_dir
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_default()
            );
            write_marker(
                &platform_dir,
                &OwnershipMarker {
                    schema: 1,
                    component_id: tool.to_string(),
                    component_kind: "runtime_tool".to_string(),
                    version: version.clone(),
                    artifact_id: None,
                    sha256: None,
                    origin: ORIGIN_LEGACY.to_string(),
                    target: capability::current_target().to_string(),
                    arch: capability::current_arch().to_string(),
                    installed_at: chrono::Utc::now().to_rfc3339(),
                },
            )
            .await?;
            upsert_entry(
                &mut manifest,
                InventoryEntry {
                    component_id: tool.to_string(),
                    component_kind: "runtime_tool".to_string(),
                    version: version.clone(),
                    origin: ORIGIN_LEGACY.to_string(),
                    artifact_id: None,
                    sha256: None,
                    path: relative,
                    active: false,
                },
            );
            entries.push(LegacyMigrationEntry {
                component_id: tool.to_string(),
                component_kind: "runtime_tool".to_string(),
                version,
                from: platform_dir.to_string_lossy().into_owned(),
                origin: ORIGIN_LEGACY.to_string(),
                verified: true,
                note,
            });
        }
    }

    write_manifest(data_dir, &manifest).await?;
    let receipt = LegacyMigrationReceipt {
        schema_version: MIGRATION_SCHEMA_VERSION,
        migrated_at: chrono::Utc::now().to_rfc3339(),
        entries,
    };
    let path = receipt_path(data_dir);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(AppCommandError::io)?;
    }
    let raw = serde_json::to_vec_pretty(&receipt).map_err(|error| {
        AppCommandError::configuration_invalid("Migration receipt is not serializable")
            .with_detail(error.to_string())
    })?;
    let temporary = path.with_extension("json.next");
    tokio::fs::write(&temporary, &raw)
        .await
        .map_err(AppCommandError::io)?;
    tokio::fs::rename(&temporary, &path)
        .await
        .map_err(AppCommandError::io)?;

    Ok(LegacyMigrationReport {
        migrated: receipt.entries.len(),
        skipped: 0,
        receipt_written: true,
    })
}

/// 定位 `<version>/<platform>` 层：Windows 平台目录名为 `win-x64` 等。
async fn legacy_platform_dir(version_dir: &Path, tool: &str) -> Option<PathBuf> {
    let platform = match capability::current_arch() {
        "x86_64" => "win-x64",
        "aarch64" => "win-arm64",
        "x86" => "win-x86",
        _ => return None,
    };
    let candidate = version_dir.join(platform);
    if candidate.is_dir() {
        return Some(candidate);
    }
    // 兼容无平台子目录的旧布局（直接版本目录）。
    let _ = tool;
    if version_dir.join("node.exe").is_file() || version_dir.join("cmd").join("git.exe").is_file()
    {
        return Some(version_dir.to_path_buf());
    }
    None
}

/// 校验旧布局：入口文件存在且版本目录可被当前架构识别。
fn verify_legacy_layout(tool: &str, platform_dir: &Path) -> (bool, String) {
    match tool {
        "git" => {
            let git_exe = platform_dir.join("cmd").join("git.exe");
            (git_exe.is_file(), "git cmd/git.exe present".to_string())
        }
        "node" => {
            let node_exe = platform_dir.join("node.exe");
            let npm_cmd = platform_dir.join("npm.cmd");
            (
                node_exe.is_file() && npm_cmd.is_file(),
                "node.exe + npm.cmd present".to_string(),
            )
        }
        "uv" => {
            let uv_exe = platform_dir.join("uv.exe");
            let uvx_exe = platform_dir.join("uvx.exe");
            (
                uv_exe.is_file() && uvx_exe.is_file(),
                "uv.exe + uvx.exe present".to_string(),
            )
        }
        _ => (false, "unknown tool".to_string()),
    }
}
