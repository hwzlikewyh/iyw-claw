//! 托管初始化状态机与持久化检查点。
//!
//! 状态机覆盖首次初始化与后续修复的完整生命周期：
//!
//! ```text
//! not_started -> resolving -> downloading -> verifying -> staging
//!             -> activating -> health_check -> ready
//!             -> degraded / retryable / blocked
//! ```
//!
//! 每次阶段迁移都会原子写入 `<root>/inventory/bootstrap-state.json`，进程崩溃或
//! 强制退出后从最后一个安全边界恢复，而不是从头重来。每个 installation 通过
//! `<root>/inventory/.bootstrap-writer.lock` 保证只有一个窗口执行 bootstrap 写
//! 操作，其余窗口订阅进度事件。

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::app_error::AppCommandError;

pub const BOOTSTRAP_STATE_SCHEMA: u32 = 1;
const STATE_FILE: &str = "bootstrap-state.json";
const WRITER_LOCK_FILE: &str = ".bootstrap-writer.lock";
const STALE_LOCK_AGE: Duration = Duration::from_secs(30 * 60);

/// 初始化阶段。`serde` 序列化为 snake_case，与前端共享。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitPhase {
    NotStarted,
    Resolving,
    Downloading,
    Verifying,
    Staging,
    Activating,
    HealthCheck,
    Ready,
    Degraded,
    Retryable,
    Blocked,
}

impl InitPhase {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            InitPhase::Ready | InitPhase::Degraded | InitPhase::Retryable | InitPhase::Blocked
        )
    }
}

/// 单个受管组件的检查点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentCheckpoint {
    pub component_id: String,
    pub component_kind: String,
    #[serde(default)]
    pub version: String,
    pub phase: InitPhase,
    pub installed: bool,
    pub active: bool,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub updated_at: String,
}

/// 整个 installation 的初始化状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapState {
    pub schema_version: u32,
    pub phase: InitPhase,
    #[serde(default)]
    pub components: Vec<ComponentCheckpoint>,
    #[serde(default)]
    pub updated_at: String,
}

impl Default for BootstrapState {
    fn default() -> Self {
        Self {
            schema_version: BOOTSTRAP_STATE_SCHEMA,
            phase: InitPhase::NotStarted,
            components: Vec::new(),
            updated_at: now_rfc3339(),
        }
    }
}

impl BootstrapState {
    pub fn component(&self, component_id: &str) -> Option<&ComponentCheckpoint> {
        self.components
            .iter()
            .find(|item| item.component_id == component_id)
    }

    pub fn set_phase(&mut self, phase: InitPhase) {
        self.phase = phase;
        self.updated_at = now_rfc3339();
    }

    pub fn upsert_component(&mut self, checkpoint: ComponentCheckpoint) {
        if let Some(existing) = self
            .components
            .iter_mut()
            .find(|item| item.component_id == checkpoint.component_id)
        {
            *existing = checkpoint;
        } else {
            self.components.push(checkpoint);
        }
        self.updated_at = now_rfc3339();
    }
}

pub fn inventory_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("inventory")
}

pub fn state_path(data_dir: &Path) -> PathBuf {
    inventory_dir(data_dir).join(STATE_FILE)
}

pub fn writer_lock_path(data_dir: &Path) -> PathBuf {
    inventory_dir(data_dir).join(WRITER_LOCK_FILE)
}

/// 读取初始化检查点；不存在时返回 `NotStarted` 默认状态。
pub async fn read_state(data_dir: &Path) -> Result<BootstrapState, AppCommandError> {
    match tokio::fs::read_to_string(state_path(data_dir)).await {
        Ok(raw) => serde_json::from_str(&raw).map_err(|error| {
            AppCommandError::configuration_invalid("Bootstrap state is corrupted")
                .with_detail(error.to_string())
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BootstrapState::default()),
        Err(error) => Err(AppCommandError::io(error)),
    }
}

/// 原子写入初始化检查点（临时文件 + rename）。
pub async fn write_state(
    data_dir: &Path,
    state: &BootstrapState,
) -> Result<(), AppCommandError> {
    let path = state_path(data_dir);
    let parent = path.parent().ok_or_else(|| {
        AppCommandError::configuration_invalid("Bootstrap inventory path is invalid")
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(AppCommandError::io)?;
    let raw = serde_json::to_vec_pretty(state).map_err(|error| {
        AppCommandError::configuration_invalid("Bootstrap state is not serializable")
            .with_detail(error.to_string())
    })?;
    let temporary = path.with_extension("json.next");
    {
        let mut file = tokio::fs::File::create(&temporary)
            .await
            .map_err(AppCommandError::io)?;
        file.write_all(&raw).await.map_err(AppCommandError::io)?;
        file.sync_all().await.map_err(AppCommandError::io)?;
    }
    tokio::fs::rename(&temporary, &path)
        .await
        .map_err(AppCommandError::io)
}

/// 单写入者锁。`acquire` 成功返回 guard；被其他窗口持有时返回 `Ok(None)`，
/// 调用方应订阅进度而非重复写入。
pub struct WriterLockGuard {
    path: PathBuf,
}

impl Drop for WriterLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub async fn acquire_writer_lock(
    data_dir: &Path,
) -> Result<Option<WriterLockGuard>, AppCommandError> {
    let path = writer_lock_path(data_dir);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(AppCommandError::io)?;
    }
    if try_create_lock(&path).await? {
        return Ok(Some(WriterLockGuard { path }));
    }
    // 锁已存在：陈旧（超时或进程已退出）则接管，否则视为其他窗口正在初始化。
    if lock_is_stale(&path).await? {
        let _ = tokio::fs::remove_file(&path).await;
        if try_create_lock(&path).await? {
            return Ok(Some(WriterLockGuard { path }));
        }
    }
    Ok(None)
}

async fn try_create_lock(path: &Path) -> Result<bool, AppCommandError> {
    let payload = serde_json::json!({
        "pid": std::process::id(),
        "startedAt": now_rfc3339(),
    });
    match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
    {
        Ok(mut file) => {
            let _ = file
                .write_all(serde_json::to_vec(&payload).unwrap_or_default().as_slice())
                .await;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(AppCommandError::io(error)),
    }
}

async fn lock_is_stale(path: &Path) -> Result<bool, AppCommandError> {
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(AppCommandError::io(error)),
    };
    let modified = metadata
        .modified()
        .map_err(|error| AppCommandError::io(error))?;
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    if age >= STALE_LOCK_AGE {
        return Ok(true);
    }
    // 进程级检查：写入者进程已退出则视为陈旧。失败时保守按年龄判定。
    let raw = match tokio::fs::read_to_string(path).await {
        Ok(raw) => raw,
        Err(_) => return Ok(false),
    };
    let pid = serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|value| value.get("pid"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default();
    if pid == 0 {
        return Ok(false);
    }
    Ok(!pid_alive(pid as u32))
}

fn pid_alive(pid: u32) -> bool {
    let mut system = sysinfo::System::new();
    let pid = sysinfo::Pid::from_u32(pid);
    system.refresh_process(pid);
    system.process(pid).is_some()
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
