//! 磁盘预检：安装前确认归档、解压展开、staging 与保留旧版本都有足够空间。

use std::path::Path;

use sysinfo::Disks;

/// 预检估算：一次安装需要的峰值磁盘占用。
#[derive(Debug, Clone, Copy)]
pub struct InstallEstimate {
    /// 归档文件字节（下载期间占用）。
    pub archive_bytes: u64,
    /// 解压后展开字节。
    pub expanded_bytes: u64,
    /// 保留旧版本需要的最小余量。
    pub retention_bytes: u64,
}

/// 预检结果，供 UI 展示可用空间。
#[derive(Debug, Clone)]
pub struct PreflightResult {
    pub required_bytes: u64,
    pub available_bytes: u64,
    pub enough: bool,
}

const HEADROOM_FACTOR: u64 = 3;
const MIN_HEADROOM_BYTES: u64 = 256 * 1024 * 1024;

/// 检查 `target_dir` 所在磁盘空间是否足够本次安装。不足时返回错误；调用方应
/// 在开始下载前调用，避免下载到一半磁盘写满。
pub fn ensure_disk_headroom(
    target_dir: &Path,
    estimate: &InstallEstimate,
) -> Result<PreflightResult, String> {
    let required = estimate
        .archive_bytes
        .saturating_add(estimate.expanded_bytes)
        .saturating_add(estimate.retention_bytes);
    let required_with_headroom = required
        .saturating_mul(HEADROOM_FACTOR)
        .max(MIN_HEADROOM_BYTES);
    let available = available_bytes_on(target_dir).unwrap_or(0);
    let enough = available >= required_with_headroom;
    if !enough {
        return Err(format!(
            "磁盘空间不足：需要约 {required_with_headroom} 字节，可用 {available} 字节"
        ));
    }
    Ok(PreflightResult {
        required_bytes: required_with_headroom,
        available_bytes: available,
        enough: true,
    })
}

/// 返回路径所在磁盘的可用字节；无法识别磁盘时返回 `None`。
pub fn available_bytes_on(path: &Path) -> Option<u64> {
    let disks = Disks::new_with_refreshed_list();
    let absolute = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    let mut best: Option<(&sysinfo::Disk, u64)> = None;
    for disk in &disks {
        let mount = disk.mount_point();
        if !absolute.starts_with(mount) {
            continue;
        }
        let depth = mount.components().count() as u64;
        if best.as_ref().map_or(true, |(_, best_depth)| depth > *best_depth) {
            best = Some((disk, depth));
        }
    }
    best.map(|(disk, _)| disk.available_space())
}
