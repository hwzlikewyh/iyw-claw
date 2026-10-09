// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What the child's fd 0, 1 or 2 is.
#[derive(Debug, Clone, Copy)]
pub enum ChildFd {
    /// A descriptor of ours, duplicated into the child's slot.
    Inherit(RawFd),
    /// `/dev/null`.
    Null,
}

pub struct SpawnSpec<'a> {
    pub program: &'a Path,
    /// `argv[1..]`; `argv[0]` is the program path.
    pub args: &'a [&'a str],
    /// The child's whole environment. Nothing of this process's own is
    /// passed on unless it is listed here.
    pub env: &'a [(String, String)],
    pub stdio: [ChildFd; 3],
    pub disclaim: bool,
    pub suspended: bool,
    /// An encoded launch requirement the image must satisfy, or the kernel
    /// kills the child at `exec`.
    pub launch_requirement: Option<&'a [u8]>,
}
