// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The kernel's name for the process on the other end of a local socket.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AuditToken(pub [u32; 8]);
