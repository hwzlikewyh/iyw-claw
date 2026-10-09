// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Where Apple keeps the applications people use, under `/System`.
pub(super) const SYSTEM_APPLICATIONS: &[&str] = &[
    "/System/Applications/",
    "/System/Cryptexes/App/System/Applications/",
    "/System/Library/CoreServices/Applications/",
];

/// The one application Apple keeps among its agents.
pub(super) const FINDER: &str = "/System/Library/CoreServices/Finder.app";
