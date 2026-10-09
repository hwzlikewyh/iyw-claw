// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// One application on the default blocklist, as every platform names it.
#[derive(Debug, Clone, Copy)]
pub struct DefaultBlock {
    /// Stable across releases: what a person's "take this one off" is
    /// remembered by, so a name added to an entry later is off with it.
    pub key: &'static str,
    /// Its product name. The few that are the system's own are named by the
    /// interface, in the person's language, by `key`.
    pub name: &'static str,
    /// Bundle identifiers.
    pub macos: &'static [&'static str],
    /// Executable file names.
    pub windows: &'static [&'static str],
    /// Executable file names.
    pub linux: &'static [&'static str],
}
