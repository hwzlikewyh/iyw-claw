//! 磁盘预检：安装前确认归档、解压展开、staging 与保留旧版本都有足够空间。

use std::path::Path;

#[cfg(not(windows))]
use sysinfo::Disks;

#[derive(Debug, Clone, Copy)]
struct DiskSpaceProbe {
    available_bytes: u64,
    total_bytes: u64,
    free_bytes: u64,
    backend: &'static str,
    ancestor_hops: usize,
}

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

const HEADROOM_FACTOR: u64 = 3;
const MIN_HEADROOM_BYTES: u64 = 256 * 1024 * 1024;

/// 检查 `target_dir` 所在磁盘空间是否足够本次安装。不足时返回错误；调用方应
/// 在开始下载前调用，避免下载到一半磁盘写满。
pub fn ensure_disk_headroom(target_dir: &Path, estimate: &InstallEstimate) -> Result<(), String> {
    let required = estimate
        .archive_bytes
        .saturating_add(estimate.expanded_bytes)
        .saturating_add(estimate.retention_bytes);
    let required_with_headroom = required
        .saturating_mul(HEADROOM_FACTOR)
        .max(MIN_HEADROOM_BYTES);
    tracing::info!(
        target: "runtime_bootstrap",
        archive_bytes = estimate.archive_bytes,
        expanded_bytes = estimate.expanded_bytes,
        retention_bytes = estimate.retention_bytes,
        required_bytes = required_with_headroom,
        target_exists = target_dir.exists(),
        "runtime disk preflight started"
    );
    let probe = probe_available_bytes(target_dir).map_err(|error| {
        tracing::error!(
            target: "runtime_bootstrap",
            error = %error,
            target_exists = target_dir.exists(),
            "runtime disk preflight probe failed"
        );
        format!("无法检测安装目录所在磁盘的可用空间：{error}")
    })?;
    tracing::info!(
        target: "runtime_bootstrap",
        backend = probe.backend,
        ancestor_hops = probe.ancestor_hops,
        available_bytes = probe.available_bytes,
        total_bytes = probe.total_bytes,
        free_bytes = probe.free_bytes,
        required_bytes = required_with_headroom,
        enough = probe.available_bytes >= required_with_headroom,
        "runtime disk preflight completed"
    );
    if probe.available_bytes < required_with_headroom {
        return Err(format!(
            "磁盘空间不足：需要约 {required_with_headroom} 字节，可用 {} 字节",
            probe.available_bytes
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn probe_available_bytes(path: &Path) -> Result<DiskSpaceProbe, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let (existing, ancestor_hops) = nearest_existing_ancestor(path)?;
    let resolved = existing
        .canonicalize()
        .map_err(|error| format!("无法解析磁盘检测路径：{error}"))?;
    let mut wide: Vec<u16> = resolved.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut available_bytes = 0_u64;
    let mut total_bytes = 0_u64;
    let mut free_bytes = 0_u64;
    let succeeded = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available_bytes,
            &mut total_bytes,
            &mut free_bytes,
        )
    };
    if succeeded == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(DiskSpaceProbe {
        available_bytes,
        total_bytes,
        free_bytes,
        backend: "windows_get_disk_free_space_ex",
        ancestor_hops,
    })
}

#[cfg(windows)]
fn nearest_existing_ancestor(path: &Path) -> Result<(&Path, usize), String> {
    let mut candidate = path;
    let mut ancestor_hops = 0;
    loop {
        match candidate.try_exists() {
            Ok(true) => return Ok((candidate, ancestor_hops)),
            Ok(false) => {}
            Err(error) => return Err(error.to_string()),
        }
        candidate = candidate
            .parent()
            .ok_or_else(|| "安装目录及其父目录均不存在".to_string())?;
        ancestor_hops += 1;
    }
}

#[cfg(not(windows))]
fn probe_available_bytes(path: &Path) -> Result<DiskSpaceProbe, String> {
    let disks = Disks::new_with_refreshed_list();
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut best: Option<(&sysinfo::Disk, u64)> = None;
    for disk in &disks {
        let mount = disk.mount_point();
        if !absolute.starts_with(mount) {
            continue;
        }
        let depth = mount.components().count() as u64;
        if best
            .as_ref()
            .map_or(true, |(_, best_depth)| depth > *best_depth)
        {
            best = Some((disk, depth));
        }
    }
    best.map(|(disk, _)| DiskSpaceProbe {
        available_bytes: disk.available_space(),
        total_bytes: disk.total_space(),
        free_bytes: disk.available_space(),
        backend: "sysinfo_mount_match",
        ancestor_hops: 0,
    })
    .ok_or_else(|| "无法识别安装目录所在的挂载点".to_string())
}
