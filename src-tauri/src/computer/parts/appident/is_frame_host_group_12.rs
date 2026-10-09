// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether the executable at `path` is the frame host, known by its file
/// name as the other hosts are. Taking a process for it lends its windows no
/// identity: each is still the application of the process drawing inside it,
/// or nobody's (see the module note).
pub fn is_frame_host(path: &str) -> bool {
    path.rsplit(['\\', '/'])
        .next()
        .is_some_and(|file| file.eq_ignore_ascii_case(FRAME_HOST))
}
