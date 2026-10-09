// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) type SecCodeRef = *const c_void;

pub(super) type SecRequirementRef = *const c_void;

pub(super) type OSStatus = i32;

pub(super) type SecCSFlags = u32;

pub(super) const K_SEC_CS_DEFAULT_FLAGS: SecCSFlags = 0;

pub(super) const K_SEC_CS_SIGNING_INFORMATION: SecCSFlags = 1 << 1;

pub(super) const K_SEC_CS_REQUIREMENT_INFORMATION: SecCSFlags = 1 << 2;

/// `csops` operations and status bits (`<kern/cs_blobs.h>`).
pub(super) const CS_OPS_STATUS: u32 = 0;

pub(super) const CS_OPS_CDHASH: u32 = 5;
