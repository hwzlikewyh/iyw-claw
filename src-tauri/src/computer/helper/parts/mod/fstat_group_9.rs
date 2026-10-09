// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(target_os = "macos")]
pub(super) fn fstat(fd: i32) -> Option<libc::stat> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: fstat on a descriptor number with a valid out-parameter.
    (unsafe { libc::fstat(fd, &mut st) } == 0).then_some(st)
}

#[cfg(target_os = "macos")]
pub(super) fn is_socket(fd: i32) -> bool {
    fstat(fd).is_some_and(|st| (st.st_mode & libc::S_IFMT) == libc::S_IFSOCK)
}

#[cfg(target_os = "macos")]
pub(super) fn same_file(a: i32, b: i32) -> bool {
    match (fstat(a), fstat(b)) {
        (Some(x), Some(y)) => x.st_dev == y.st_dev && x.st_ino == y.st_ino,
        _ => false,
    }
}
