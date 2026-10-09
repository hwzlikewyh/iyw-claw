// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The application `pid` runs, read off the process and then checked to be
/// the process the window list named — a pid reused in between would lend
/// the window another application's identity — and named as the Finder names
/// it (`owner` is the window list's name for the process; see `appident`).
/// Unidentified (no bundle, no path, the window list's name) when it is no
/// application, or when that cannot be told.
#[cfg(target_os = "macos")]
pub(in crate::computer::helper) fn identified(
    pid: u32,
    started_at: Option<u64>,
    owner: &str,
) -> RawApp {
    let identity = started_at
        .and_then(|_| crate::computer::appident::identify(pid))
        .filter(|_| process_start(pid) == started_at);
    let unidentified = RawApp {
        pid,
        name: owner.to_string(),
        bundle_id: None,
        path: None,
        active: false,
        started_at,
    };
    match identity {
        Some(identity) => RawApp {
            name: identity.name(owner),
            bundle_id: Some(identity.bundle_id),
            path: Some(identity.path),
            ..unidentified
        },
        None => unidentified,
    }
}
