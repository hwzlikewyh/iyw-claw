//! 文件系统库存清单、受管目录 ownership marker 与升级保护摘要。
//!
//! - 每个受管组件版本目录写入 `.iyw-claw-marker.json`，包含 schema、组件 ID、
//!   版本、artifact ID、SHA-256、target/arch、安装时间与来源。
//! - `<root>/inventory/manifest.json` 是小型库存快照，启动热路径只读它，不扫描
//!   整个目录树。
//! - `digest_managed_root` 计算持久区摘要，供应用更新前后对比（远端 E2E）。
//! - 用户目录（`config`、`data`、`logs`、`skills/user`）永不写入受管 marker。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

pub const INVENTORY_SCHEMA_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "manifest.json";
const MARKER_FILE: &str = ".iyw-claw-marker.json";
const PENDING_ACTIVATIONS_FILE: &str = "pending-activations.json";

/// 受管目录 ownership marker。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OwnershipMarker {
    pub schema: u32,
    pub component_id: String,
    pub component_kind: String,
    pub version: String,
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    pub target: String,
    pub arch: String,
    pub installed_at: String,
    /// `managed`（可信 artifact）| `legacy`（旧目录导入，无可信摘要）。
    pub origin: String,
}

/// 库存清单条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryEntry {
    pub component_id: String,
    pub component_kind: String,
    pub version: String,
    pub origin: String,
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    /// 相对 `<root>` 的路径。
    pub path: String,
    pub active: bool,
}

/// 小型库存快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryManifest {
    pub schema_version: u32,
    pub generation: u64,
    pub updated_at: String,
    #[serde(default)]
    pub entries: Vec<InventoryEntry>,
}

impl Default for InventoryManifest {
    fn default() -> Self {
        Self {
            schema_version: INVENTORY_SCHEMA_VERSION,
            generation: 0,
            updated_at: chrono::Utc::now().to_rfc3339(),
            entries: Vec::new(),
        }
    }
}

/// 待激活标记（活跃会话存活时不切换 Agent，记录为 pending）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingActivation {
    pub component_id: String,
    pub component_kind: String,
    pub version: String,
    pub created_at: String,
}

pub fn inventory_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("inventory")
}

pub fn manifest_path(data_dir: &Path) -> PathBuf {
    inventory_dir(data_dir).join(MANIFEST_FILE)
}

pub fn marker_path(component_dir: &Path) -> PathBuf {
    component_dir.join(MARKER_FILE)
}

pub fn pending_activations_path(data_dir: &Path) -> PathBuf {
    inventory_dir(data_dir).join(PENDING_ACTIVATIONS_FILE)
}

/// 读取库存清单；缺失时返回默认空清单。
pub async fn read_manifest(data_dir: &Path) -> Result<InventoryManifest, AppCommandError> {
    match tokio::fs::read_to_string(manifest_path(data_dir)).await {
        Ok(raw) => serde_json::from_str(&raw).map_err(|error| {
            AppCommandError::configuration_invalid("Inventory manifest is corrupted")
                .with_detail(error.to_string())
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(InventoryManifest::default()),
        Err(error) => Err(AppCommandError::io(error)),
    }
}

/// 原子写入库存清单。
pub async fn write_manifest(
    data_dir: &Path,
    manifest: &InventoryManifest,
) -> Result<(), AppCommandError> {
    let path = manifest_path(data_dir);
    let parent = path.parent().ok_or_else(|| {
        AppCommandError::configuration_invalid("Inventory path is invalid")
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(AppCommandError::io)?;
    let raw = serde_json::to_vec_pretty(manifest).map_err(|error| {
        AppCommandError::configuration_invalid("Inventory manifest is not serializable")
            .with_detail(error.to_string())
    })?;
    let temporary = path.with_extension("json.next");
    tokio::fs::write(&temporary, &raw)
        .await
        .map_err(AppCommandError::io)?;
    tokio::fs::rename(&temporary, &path)
        .await
        .map_err(AppCommandError::io)
}

/// 写入组件目录的 ownership marker。
pub async fn write_marker(
    component_dir: &Path,
    marker: &OwnershipMarker,
) -> Result<(), AppCommandError> {
    tokio::fs::create_dir_all(component_dir)
        .await
        .map_err(AppCommandError::io)?;
    let raw = serde_json::to_vec_pretty(marker).map_err(|error| {
        AppCommandError::configuration_invalid("Ownership marker is not serializable")
            .with_detail(error.to_string())
    })?;
    let path = marker_path(component_dir);
    let temporary = path.with_extension("json.next");
    tokio::fs::write(&temporary, &raw)
        .await
        .map_err(AppCommandError::io)?;
    tokio::fs::rename(&temporary, &path)
        .await
        .map_err(AppCommandError::io)
}

/// 读取组件目录的 ownership marker。
pub async fn read_marker(component_dir: &Path) -> Option<OwnershipMarker> {
    let raw = tokio::fs::read_to_string(marker_path(component_dir))
        .await
        .ok()?;
    serde_json::from_str(&raw).ok()
}

/// marker 与期望完全匹配（组件 ID、版本、artifact ID、SHA-256 一致）。
pub fn marker_matches(marker: &OwnershipMarker, expected: &OwnershipMarker) -> bool {
    marker.component_id == expected.component_id
        && marker.component_kind == expected.component_kind
        && marker.version == expected.version
        && marker.artifact_id == expected.artifact_id
        && marker.sha256 == expected.sha256
        && marker.origin == expected.origin
}

/// 升级保护摘要：对库存清单与所有 active pointer 做 SHA-256。
/// 只读小型文件，不扫描目录树。应用更新前后比较此摘要必须不变。
pub async fn digest_managed_root(data_dir: &Path) -> Result<String, AppCommandError> {
    let manifest = read_manifest(data_dir).await?;
    let mut hasher = sha2::Sha256::new();
    let manifest_raw = serde_json::to_vec(&manifest).map_err(|error| {
        AppCommandError::configuration_invalid("Inventory manifest is not serializable")
            .with_detail(error.to_string())
    })?;
    hasher.update(&manifest_raw);
    // active pointer（current.json）纳入摘要：指针变化意味着受管版本切换。
    for entry in &manifest.entries {
        if !entry.active {
            continue;
        }
        let pointer = data_dir
            .join(&entry.path)
            .join("current.json");
        if let Ok(raw) = tokio::fs::read(&pointer).await {
            hasher.update(&pointer.to_string_lossy().as_bytes());
            hasher.update(&raw);
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 读取待激活列表。
pub async fn read_pending_activations(
    data_dir: &Path,
) -> Result<Vec<PendingActivation>, AppCommandError> {
    match tokio::fs::read_to_string(pending_activations_path(data_dir)).await {
        Ok(raw) => serde_json::from_str(&raw).map_err(|error| {
            AppCommandError::configuration_invalid("Pending activations are corrupted")
                .with_detail(error.to_string())
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(AppCommandError::io(error)),
    }
}

/// 原子写入待激活列表。
pub async fn write_pending_activations(
    data_dir: &Path,
    pending: &[PendingActivation],
) -> Result<(), AppCommandError> {
    let path = pending_activations_path(data_dir);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(AppCommandError::io)?;
    }
    let raw = serde_json::to_vec_pretty(pending).map_err(|error| {
        AppCommandError::configuration_invalid("Pending activations are not serializable")
            .with_detail(error.to_string())
    })?;
    let temporary = path.with_extension("json.next");
    tokio::fs::write(&temporary, &raw)
        .await
        .map_err(AppCommandError::io)?;
    tokio::fs::rename(&temporary, &path)
        .await
        .map_err(AppCommandError::io)
}

/// 从 manifest 条目生成组件目录路径（相对 `<root>`）。
pub fn entry_directory(entry: &InventoryEntry) -> PathBuf {
    PathBuf::from(&entry.path)
}

/// 更新 manifest 中的单个条目并递增 generation。
pub fn upsert_entry(manifest: &mut InventoryManifest, entry: InventoryEntry) {
    if let Some(existing) = manifest
        .entries
        .iter_mut()
        .find(|item| item.component_id == entry.component_id && item.version == entry.version)
    {
        *existing = entry;
    } else {
        manifest.entries.push(entry);
    }
    manifest.generation = manifest.generation.saturating_add(1);
    manifest.updated_at = chrono::Utc::now().to_rfc3339();
}

/// 汇总 manifest 条目为 `component_id -> version` 映射，供热路径快速查询。
pub fn active_versions(manifest: &InventoryManifest) -> BTreeMap<String, String> {
    manifest
        .entries
        .iter()
        .filter(|entry| entry.active)
        .map(|entry| (entry.component_id.clone(), entry.version.clone()))
        .collect()
}
