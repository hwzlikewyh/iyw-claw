// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(not(target_os = "macos"))]
pub(super) fn open_channel() -> Result<Channel, (i32, String)> {
    Ok(Channel {
        raw: RawChannel::Stdio,
        peer: PeerCheck::NotApplicable,
        guard: None,
    })
}
