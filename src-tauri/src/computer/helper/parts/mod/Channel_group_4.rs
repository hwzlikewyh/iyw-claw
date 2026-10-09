// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) struct Channel {
    pub(in crate::computer::helper) raw: RawChannel,
    pub(in crate::computer::helper) peer: PeerCheck,
    pub(in crate::computer::helper) guard: Option<PeerGuard>,
}
