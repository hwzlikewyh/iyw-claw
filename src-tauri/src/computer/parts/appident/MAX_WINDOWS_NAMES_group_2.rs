// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Windows: how many applications' names are kept (see the module note).
/// Past that they are all read again, as they are needed.
#[cfg(windows)]
pub(super) const MAX_WINDOWS_NAMES: usize = 256;

/// Windows: how long a listing waits for an application's name before it
/// goes by its file name for now (see `windows_name`). The Start menu takes
/// a fifth of a second at worst when it is well.
#[cfg(windows)]
pub(super) const NAME_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// Windows: once a name has not come in time, how long no listing waits for
/// another — a shell slow to name one application is slow for all.
#[cfg(windows)]
pub(super) const NAME_SLOW: std::time::Duration = std::time::Duration::from_secs(30);

/// Windows: the most names read at once. A read the shell never answers
/// keeps its thread; past this many, applications go by their file names
/// until one comes back, rather than a thread more each.
#[cfg(windows)]
pub(super) const MAX_NAME_READERS: usize = 4;
