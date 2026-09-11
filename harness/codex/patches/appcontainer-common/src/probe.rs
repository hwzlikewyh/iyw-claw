// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Read-only fallback-detector probe.
//!
//! This module wraps [`crate::fallback_detector`] in a serde-friendly
//! surface so the SDK can invoke `wxc-exec --probe` and learn which
//! isolation tier would be selected on the current machine without
//! actually spawning a sandbox.
//!
//! The probe must have no side effects: it does not write logs, modify
//! the filesystem, or spawn child processes.

use serde::Serialize;

use crate::fallback_detector::{self, FallbackError};
use wxc_common::models::ContainerPolicy;
use wxc_common::ui_policy::EffectiveUiRestrictions;

/// JSON output emitted by `wxc-exec --probe`.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProbeOutput {
    /// Selected tier (omitted when the detector returned an error).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<&'static str>,
    /// True when the selected tier needs DACL deny-augmentation on host
    /// paths. Omitted when the detector returned an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_dacl_augmentation: Option<bool>,
    /// Operator-visible degradation warnings — one per tier fall-through.
    pub warnings: Vec<String>,
    /// Raw machine probes, independent of the policy argument.
    pub probes: ProbeFacts,
    /// Detector error message (only set when detection failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Raw machine facts gathered prior to running tier selection.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProbeFacts {
    /// `Experimental_CreateProcessInSandbox` is resolvable.
    pub base_container_api_present: bool,
    /// `bfscfg.exe` is on disk in `%SystemRoot%\System32`.
    ///
    /// Always `false` when [`Self::bfs_compiled_in`] is `false`, because
    /// `find_bfscfg_exe` returns `Ok(None)` unconditionally with the
    /// `tier2_bfs` feature off — i.e. this field reports what the
    /// detector itself would see, not what is on disk.
    pub bfscfg_present: bool,
    /// Whether this binary was compiled with the `tier2_bfs` Cargo
    /// feature. When `false`, Tier 2 (AppContainer + BFS) is
    /// unreachable: the detector falls through to Tier 3 on any host
    /// that would otherwise select T2, and the `bfscfg.exe` spawn site
    /// is itself gated. Harnesses on hang-prone hosts (e.g. Windows
    /// 11 25H2 where `bfscfg.exe` locks `bfs.sys`) should refuse to
    /// run a binary that reports `true` here.
    pub bfs_compiled_in: bool,
    /// Whether the BaseContainer (Tier 1) tier can enforce
    /// `filesystem.deniedPaths` on this host (the `SANDBOX_CAP_FS_DENY` bit
    /// from `Experimental_QuerySandboxSupport`). `false` on builds where deny
    /// support has not yet shipped, where `deniedPaths` is rejected at launch.
    /// Tier 3 (AppContainer + DACL) enforces `deniedPaths` via DENY ACEs
    /// regardless of this bit; it is meaningful only for the BaseContainer tier.
    pub base_container_supports_deny_paths: bool,
    /// Whether the in-proc IsolationSession service can be activated on this
    /// host. Always `false` here — `appcontainer_common` has no dependency on
    /// the isolation-session backend; `wxc-exec --probe` overrides it when
    /// that backend is compiled in.
    pub isolation_session_available: bool,
    /// Whether Hyperlight (WHP micro-VM) is available on this host. Always
    /// `false` here — overridden by `wxc-exec --probe` when the hyperlight
    /// feature is compiled in and WHP is loadable.
    pub hyperlight_available: bool,
    /// Platform-agnostic UI restrictions this host can enforce.
    pub ui_capabilities: UiCapabilitySupport,
}

/// Host support for enforcing sandbox UI restrictions.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UiCapabilitySupport {
    /// Whether the host can block reads from the clipboard.
    pub can_block_clipboard_read: bool,
    /// Whether the host can block writes to the clipboard.
    pub can_block_clipboard_write: bool,
    /// Whether the host can block synthetic keyboard/mouse input.
    pub can_block_input_injection: bool,
    /// Whether the host can block input method / IME changes.
    pub can_block_input_method_changes: bool,
    /// Whether the host can block access to external UI object handles.
    pub can_block_external_ui_objects: bool,
    /// Whether the host can block access to global UI namespaces.
    pub can_block_global_ui_namespace: bool,
    /// Whether the host can block desktop switching.
    pub can_block_desktop_switching: bool,
    /// Whether the host can block logoff or shutdown requests.
    pub can_block_logoff_or_shutdown: bool,
    /// Whether the host can block system parameter changes.
    pub can_block_system_parameter_changes: bool,
    /// Whether the host can block display settings changes.
    pub can_block_display_settings_changes: bool,
}

impl From<EffectiveUiRestrictions> for UiCapabilitySupport {
    fn from(value: EffectiveUiRestrictions) -> Self {
        Self {
            can_block_clipboard_read: value.block_clipboard_read,
            can_block_clipboard_write: value.block_clipboard_write,
            can_block_input_injection: value.block_input_injection,
            can_block_input_method_changes: value.block_input_method_changes,
            can_block_external_ui_objects: value.block_external_ui_objects,
            can_block_global_ui_namespace: value.block_global_ui_namespace,
            can_block_desktop_switching: value.block_desktop_switching,
            can_block_logoff_or_shutdown: value.block_logoff_or_shutdown,
            can_block_system_parameter_changes: value.block_system_parameter_changes,
            can_block_display_settings_changes: value.block_display_settings_changes,
        }
    }
}

/// Run the fallback detector against `policy` and return a JSON-shaped
/// summary. The detector is always asked to prefer BaseContainer (Tier 1).
pub fn run_probe(policy: &ContainerPolicy) -> ProbeOutput {
    let probes = ProbeFacts {
        base_container_api_present:
            crate::base_container_runner::BaseContainerRunner::is_base_container_api_present()
                .is_ok(),
        bfscfg_present: fallback_detector::find_bfscfg_exe()
            .ok()
            .flatten()
            .is_some(),
        bfs_compiled_in: cfg!(feature = "tier2_bfs"),
        base_container_supports_deny_paths:
            crate::base_container_runner::BaseContainerRunner::base_container_supports_deny_paths(),
        isolation_session_available: false,
        hyperlight_available: false,
        ui_capabilities: crate::job_object::supported_ui_restrictions().into(),
    };
    match fallback_detector::detect(policy, /* prefer_base_container */ true) {
        Ok(decision) => ProbeOutput {
            tier: Some(decision.tier.as_str()),
            needs_dacl_augmentation: Some(decision.needs_dacl_augmentation),
            warnings: decision.warnings,
            probes,
            error: None,
        },
        Err(e) => ProbeOutput {
            tier: None,
            needs_dacl_augmentation: None,
            warnings: vec![],
            probes,
            error: Some(format_fallback_error(&e)),
        },
    }
}

fn format_fallback_error(e: &FallbackError) -> String {
    match e {
        FallbackError::DaclFallbackDisabled => {
            "DACL fallback required but fallback.allowDaclMutation is false".to_string()
        }
        FallbackError::WriteDacUnavailable { path, reason } => {
            format!("WRITE_DAC unavailable on path {}: {reason}", path.display())
        }
        FallbackError::SystemRootUnresolved { reason } => {
            format!("Could not resolve Windows system directory: {reason}")
        }
    }
}

/// Serialize a [`ProbeOutput`] as pretty-printed JSON.
///
/// Returns `Err` only if the underlying serializer fails — in practice
/// this should be infallible for the well-formed `ProbeOutput` we
/// produce, but we surface the error rather than panic.
pub fn to_json_pretty(output: &ProbeOutput) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(output)
}
