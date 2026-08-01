//! per-session 独占锁：防止两个窗口/进程同时写同一 Agent 配置。
//!
//! 锁文件放在该 agent 的 profile 根目录（与受控配置同目录），
//! 通过 `create_new` 原子创建实现互斥，带超时重试上限；
//! Drop 时自动清理。锁文件不进入任何配置内容。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::models::agent::AgentType;

use super::ReconcileError;

const LOCK_FILE_NAME: &str = ".iyw-claw.session-config.lock";
const LOCK_ATTEMPT_COUNT: u32 = 50;
const LOCK_RETRY_DELAY_MS: u64 = 200;

/// 持有的会话配置锁；Drop 时释放。
#[derive(Debug)]
pub struct SessionLockGuard {
    path: PathBuf,
}

impl Drop for SessionLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// 取得指定 agent 的 per-session 独占锁。
///
/// 在 `profile_root` 下创建锁文件；若已有锁则轮询等待，超过
/// [`LOCK_ATTEMPT_COUNT`] 次仍未拿到即失败（阻止并行写同一配置）。
pub fn acquire_session_lock(
    agent: AgentType,
    profile_root: &Path,
) -> Result<SessionLockGuard, ReconcileError> {
    fs::create_dir_all(profile_root)
        .map_err(|error| ReconcileError::Failed(format!("create profile dir: {error}")))?;
    let lock_path = profile_root.join(LOCK_FILE_NAME);
    let deadline = Instant::now() + Duration::from_millis(LOCK_RETRY_DELAY_MS * u64::from(LOCK_ATTEMPT_COUNT));
    let mut attempt: u32 = 0;
    loop {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                use std::io::Write;
                let _ = writeln!(file, "{}", std::process::id());
                return Ok(SessionLockGuard { path: lock_path });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if Instant::now() >= deadline || attempt >= LOCK_ATTEMPT_COUNT {
                    return Err(ReconcileError::LockTimeout(format!(
                        "agent {agent:?} config lock held at {}",
                        lock_path.display()
                    )));
                }
                attempt += 1;
                std::thread::sleep(Duration::from_millis(LOCK_RETRY_DELAY_MS));
            }
            Err(error) => {
                return Err(ReconcileError::Failed(format!(
                    "create lock file at {}: {error}",
                    lock_path.display()
                )));
            }
        }
    }
}
