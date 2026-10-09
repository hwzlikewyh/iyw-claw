// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// macOS: join each window with its application as the helper reads it off
/// the owning process now (`appident`) — once per process per listing, and
/// never remembered past it: reading it is a few system calls and a small
/// file, and a process that has since run another program (`exec` keeps the
/// pid and the start time) is that program now.
#[cfg(target_os = "macos")]
pub(super) fn join_identified(windows: Vec<RawWindow>, stamps: Vec<Option<u64>>) -> Vec<RawWindow> {
    let mut seen: HashMap<(u32, Option<u64>), RawApp> = HashMap::new();
    windows
        .into_iter()
        .zip(stamps)
        .map(|(mut window, started_at)| {
            window.app = seen
                .entry((window.pid, started_at))
                .or_insert_with(|| identified(window.pid, started_at, &window.app.name))
                .clone();
            window
        })
        .collect()
}
