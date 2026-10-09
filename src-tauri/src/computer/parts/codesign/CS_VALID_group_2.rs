// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub const CS_VALID: u32 = 0x0000_0001;

pub const CS_GET_TASK_ALLOW: u32 = 0x0000_0004;

pub const CS_RUNTIME: u32 = 0x0001_0000;

pub const CS_DEBUGGED: u32 = 0x1000_0000;
