// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! AppContainer + BaseContainer backend family, including the
//! T1/T2/T3 isolation-tier fallback ladder and the Windows-only
//! support modules they depend on (job objects, BFS policy,
//! network/proxy plumbing, sandbox tracking, launch diagnostics).
//!
//! All modules are Windows-only. The crate links unconditionally so
//! `wxc-exec` (which always targets Windows) can depend on it
//! without feature gates, while cross-platform consumers of
//! `wxc_common` are unaffected by AppContainer code.

#[cfg(target_os = "windows")]
pub mod appcontainer_runner;
#[cfg(target_os = "windows")]
mod base_container_helpers;
#[cfg(target_os = "windows")]
pub mod base_container_runner;
#[cfg(target_os = "windows")]
pub mod capture_output;
#[cfg(target_os = "windows")]
pub mod dispatcher;
#[cfg(target_os = "windows")]
pub mod fallback_detector;
#[cfg(target_os = "windows")]
pub mod filesystem_bfs;
#[cfg(target_os = "windows")]
pub mod guarded_capture;
#[cfg(target_os = "windows")]
pub mod job_object;
#[cfg(target_os = "windows")]
pub mod launch_diagnostics;
#[cfg(target_os = "windows")]
pub mod network_manager;
#[cfg(target_os = "windows")]
mod network_policy_helpers;
#[cfg(target_os = "windows")]
pub mod probe;
#[cfg(target_os = "windows")]
pub mod process_mitigation;
#[cfg(target_os = "windows")]
pub mod proxy_coordinator;
#[cfg(target_os = "windows")]
pub mod sandbox_tracking;
/// Working-directory resolution for both Windows launch paths. Deliberately
/// **not** `cfg`-gated: the mapping is pure, and keeping it portable means its
/// regression tests (notably "never resolve to a `NULL` cwd") run on every CI
/// lane rather than only the Windows one.
pub mod working_directory;
