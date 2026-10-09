// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl std::fmt::Debug for AuditToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AuditToken(pid {}, version {})",
            self.pid(),
            self.pid_version()
        )
    }
}

impl AuditToken {
    /// `audit_token_to_pid`: the sixth word.
    pub fn pid(&self) -> u32 {
        self.0[5]
    }

    /// `audit_token_to_pidversion`: the eighth word. Moves every time a pid
    /// is handed out again.
    pub fn pid_version(&self) -> u32 {
        self.0[7]
    }
}
