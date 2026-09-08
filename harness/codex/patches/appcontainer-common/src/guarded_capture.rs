// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Dependency-injection boundary for the guarded WPR capture fallback.
//!
//! `appcontainer_common` implements the legacy containment tiers (BaseContainer
//! SBOX, AppContainer + BFS, AppContainer + DACL) that a host without the
//! native V2 PSEC + Learning Mode APIs still needs `captureDenials` on.
//! Elevated WPR capture lives in `plm` (the host's guarded PLM tool), and
//! `appcontainer_common` MUST NOT depend on `plm` directly: `plm` links the
//! Windows ETL decoder (`learning_mode_windows`) and elevation/pipe machinery
//! that is unrelated to this crate's job, and the crate-layering rule
//! (backend-support crates don't cross-depend on one another) forbids it.
//!
//! Instead, this module defines the minimal traits a legacy-tier runner needs
//! to start and stop a guarded WPR capture scoped to its own sandboxed process
//! tree. `mxc_engine` (which already depends on `plm` for the executor
//! binaries' guarded-PLM lifecycle) implements them by adapting
//! `plm::elevated::{start_guarded_session_with_executable, GuardedSession}`,
//! and hands the concrete factory to the dispatcher only when it explicitly
//! opts a request into the fallback (see
//! `dispatcher::dispatch_with_fallback_and_capture` /
//! `dispatcher::spawn_with_fallback_and_capture`) — a runner never picks up
//! guarded capture silently.

use std::path::Path;

use learning_mode_core::AnalysisResult;
use wxc_common::models::{
    CaptureDenialsErrorOutput, FailurePhase, SandboxOutputMetadata, ScriptResponse,
};

/// Error returned when an injected guarded-capture implementation cannot
/// transfer retained ETL requested by the caller.
pub const RETAIN_ETL_UNSUPPORTED_MSG: &str =
    "processContainer.captureDenials.retainEtl is not supported by the configured \
     guarded-WPR capture provider";

/// Outcome of [`GuardedCaptureSession::stop_analyzed_with_trace`].
///
/// The process-scoped [`AnalysisResult`] is always present: an analysis failure
/// is reported as the method's `Err`, never as a value here. `trace_retention`
/// independently reports whether the sealed ETL was transferred to the
/// requested destination — a retention failure that occurs *after* a successful
/// analysis is carried as data so the actionable denials JSON can still be
/// published while the retention failure is surfaced separately.
#[derive(Debug)]
pub struct AnalyzedTrace {
    /// The bounded, process-scoped analysis of the guarded WPR trace.
    pub analysis: AnalysisResult,
    /// Whether the sealed ETL reached the requested destination. `Ok(())` means
    /// the ETL is persisted there; `Err` carries the retention failure while the
    /// `analysis` above remains valid.
    pub trace_retention: Result<(), String>,
}

/// A live guarded WPR capture session scoped to one sandboxed process tree.
///
/// Implementations own the elevated PLM child connection. [`stop_analyzed`]
/// asks the guardian to stop the host-wide WPR trace and decode it, returning
/// the bounded, process-scoped [`AnalysisResult`]. When the caller explicitly
/// requests `retainEtl`, implementations that support trace transfer may also
/// return the sealed ETL through [`stop_analyzed_with_trace`].
///
/// [`stop_analyzed`]: GuardedCaptureSession::stop_analyzed
pub trait GuardedCaptureSession: Send {
    /// Duplicates and attaches the caller's sandbox job and still-owned
    /// suspended root process in the elevated guardian. Both values must be
    /// HANDLEs owned by the authenticated unelevated process.
    fn attach_process_tree(
        &mut self,
        job_handle: usize,
        root_process_handle: usize,
    ) -> Result<(), String>;

    /// Stops the owned WPR trace and securely discards its raw ETL without
    /// analysis. Used when job attachment, sandbox launch, or sandbox
    /// termination fails.
    ///
    /// This method must not return, on either success or error, until the
    /// elevated guardian has terminated and released every duplicated sandbox
    /// handle. Runners rely on that guarantee before allowing firewall,
    /// filesystem, and DACL enforcement guards to drop.
    fn discard(&mut self) -> Result<(), String>;

    /// Stops the guarded capture and analyzes it against exact process
    /// generations: the guardian-attested root handle lifetime plus descendants
    /// reconciled from retained handles and job membership accounting.
    ///
    /// # Errors
    ///
    /// Returns a human-readable message if the guardian connection is gone,
    /// the stop/analyze round trip fails, or the guardian reports a decode
    /// error.
    fn stop_analyzed(&mut self) -> Result<AnalysisResult, String>;

    /// Stops and analyzes the guarded capture, and transfers the sealed ETL to
    /// `trace_destination` when the caller explicitly requests ETL retention.
    ///
    /// # Errors
    ///
    /// `Err` represents an *analysis* failure. A trace-transfer failure that
    /// occurs after a successful analysis is NOT an error here: it is carried in
    /// [`AnalyzedTrace::trace_retention`] so the decoded denials are never
    /// discarded.
    fn stop_analyzed_with_trace(
        &mut self,
        trace_destination: &std::path::Path,
    ) -> Result<AnalyzedTrace, String>;
}

/// Starts a [`GuardedCaptureSession`] for the calling (unelevated) process.
///
/// Implementations are constructed by a higher layer (`mxc_engine`) that can
/// depend on `plm`; `appcontainer_common` only ever sees the trait object.
pub trait GuardedCaptureFactory: Send + Sync {
    /// Whether this factory can transfer the sealed ETL when `retainEtl` is
    /// requested. Implementations that only support analysis keep the default.
    fn allows_trace_transfer(&self) -> bool {
        false
    }

    /// Starts a new guarded WPR capture session.
    ///
    /// `owner_pid` is the calling (unelevated) process's own OS process id —
    /// used by the elevated guardian to authenticate the connection — **not**
    /// the sandboxed child's pid. The child identity and lifetime are attested
    /// later from its duplicated process handle; no caller-supplied child PID
    /// or timestamp is trusted.
    ///
    /// # Errors
    ///
    /// Returns a human-readable message on failure (elevation refused,
    /// guardian unreachable, a WPR session is already active, etc.). The
    /// caller must terminate the still-suspended sandboxed child on failure
    /// rather than resume it, so no active trace is ever left behind.
    fn start(&self, owner_pid: u32) -> Result<Box<dyn GuardedCaptureSession>, String>;
}

pub(crate) fn release_after_termination_failure(
    mut session: Box<dyn GuardedCaptureSession>,
) -> Result<(), String> {
    session.discard()
}

/// Fails closed when `retainEtl` is requested but cannot be honored.
///
/// ETL retention is satisfied either by a native capture path that seals its
/// own ETL (`native_capture_retains_etl` — BaseContainer's PSEC/V2 path) or by a
/// guarded-WPR provider that can transfer the sealed ETL
/// (`provider_allows_trace_transfer`). When retention is requested and neither
/// holds, the request is rejected with [`RETAIN_ETL_UNSUPPORTED_MSG`].
///
/// Centralized so both the AppContainer fallback tiers (which have no native
/// capture path and always pass `native_capture_retains_etl = false`) and the
/// BaseContainer runner (which passes `true` when its native PSEC/V2 capture is
/// selected, making the guarded-provider capability irrelevant) share one gate.
pub fn validate_retain_etl_supported(
    retain_etl: bool,
    provider_allows_trace_transfer: bool,
    native_capture_retains_etl: bool,
) -> Result<(), ScriptResponse> {
    if retain_etl && !native_capture_retains_etl && !provider_allows_trace_transfer {
        return Err(ScriptResponse {
            failure_phase: FailurePhase::BackendUnavailable,
            ..ScriptResponse::error(RETAIN_ETL_UNSUPPORTED_MSG)
        });
    }
    Ok(())
}

/// Structured output metadata plus the teardown status produced by finalizing a
/// guarded WPR capture. Both legacy-tier runners (`appcontainer_runner`,
/// `base_container_runner`) consume this so the analysis-success/trace-failure
/// and trace-success/JSON-failure transitions live in exactly one place and
/// cannot drift between the two runners.
#[derive(Debug)]
pub struct GuardedCaptureFinalization {
    /// Structured output metadata to store on the sandbox process, if any.
    pub metadata: Option<SandboxOutputMetadata>,
    /// Teardown status to thread through the runner's `wait()`. `Ok(())` iff the
    /// actionable denials JSON was written and no retention was requested (or
    /// retention succeeded); a retention-only failure after a successful
    /// analysis returns `Err` while still publishing the JSON via `metadata`
    /// (mirroring the native capture path).
    pub result: Result<(), String>,
}

/// How the caller asked the guarded capture to be stopped, plus — for the
/// retention case — the transfer destination. Selects `stop_analyzed` vs
/// `stop_analyzed_with_trace` inside [`finalize_guarded_capture`].
pub enum GuardedStop<'a> {
    /// `retainEtl` not requested: stop and analyze only.
    AnalyzeOnly,
    /// `retainEtl` requested: stop, analyze, and transfer the sealed ETL to
    /// `destination`.
    AnalyzeAndRetain { destination: &'a Path },
}

/// Stops a guarded WPR capture and turns its outcome into the actionable denials
/// JSON plus structured metadata, in one shared place for both legacy-tier
/// runners.
///
/// - Analysis failure ⇒ no JSON, `metadata: None`, `result: Err`.
/// - Analysis success, no retention ⇒ JSON written, success metadata,
///   `result: Ok`.
/// - Analysis success, retention success ⇒ JSON written with `etlPath` set,
///   `result: Ok`.
/// - Analysis success, retention failure ⇒ JSON written (actionable success,
///   `etlPath = None`) AND `capture_denials_error` describing the retention
///   failure with an empty ETL path (nothing was persisted); `result: Err`.
/// - Analysis + retention success but the JSON write fails ⇒ `etlPath` is
///   preserved in `capture_denials_error` so the caller can find/delete the
///   retained ETL; `result: Err`.
pub fn finalize_guarded_capture(
    session: &mut dyn GuardedCaptureSession,
    output_path: Option<&Path>,
    stop: GuardedStop<'_>,
    exit_code: i32,
) -> GuardedCaptureFinalization {
    match stop {
        GuardedStop::AnalyzeOnly => {
            finalize_analysis(session.stop_analyzed(), output_path, exit_code, None)
        }
        GuardedStop::AnalyzeAndRetain { destination } => {
            match session.stop_analyzed_with_trace(destination) {
                Ok(AnalyzedTrace {
                    analysis,
                    trace_retention,
                }) => finalize_analysis(
                    Ok(analysis),
                    output_path,
                    exit_code,
                    Some((destination, trace_retention)),
                ),
                // Analysis failure: nothing decoded, nothing persisted.
                Err(error) => finalize_analysis(Err(error), output_path, exit_code, None),
            }
        }
    }
}

/// Core of [`finalize_guarded_capture`]: publishes the denials JSON and
/// computes the metadata / teardown status from the analysis result and the
/// optional retention outcome (`(destination, transfer status)`).
fn finalize_analysis(
    analysis: Result<AnalysisResult, String>,
    output_path: Option<&Path>,
    exit_code: i32,
    retention: Option<(&Path, Result<(), String>)>,
) -> GuardedCaptureFinalization {
    let analysis = match analysis {
        Ok(analysis) => analysis,
        Err(error) => {
            return GuardedCaptureFinalization {
                metadata: None,
                result: Err(format!(
                    "captureDenials failed to stop and analyze the guarded WPR session: {error}"
                )),
            };
        }
    };
    let Some(output_path) = output_path else {
        return GuardedCaptureFinalization {
            metadata: None,
            result: Err("captureDenials internal output path was not initialized".to_string()),
        };
    };
    match crate::capture_output::write_denials_document(analysis, exit_code, output_path) {
        Ok(mut success) => match retention {
            // No retention requested, or retention succeeded: publish success.
            None | Some((_, Ok(()))) => {
                if let Some((destination, Ok(()))) = retention {
                    success.etl_path = Some(destination.to_string_lossy().into_owned());
                }
                GuardedCaptureFinalization {
                    metadata: Some(SandboxOutputMetadata {
                        capture_denials: Some(success),
                        capture_denials_error: None,
                    }),
                    result: Ok(()),
                }
            }
            // Retention requested but the transfer failed: publish the actionable
            // JSON success (etlPath stays None — nothing persisted) and surface
            // the retention failure with an empty ETL path.
            Some((_, Err(retention_error))) => {
                let message =
                    format!("captureDenials could not retain the sealed ETL: {retention_error}");
                GuardedCaptureFinalization {
                    metadata: Some(SandboxOutputMetadata {
                        capture_denials: Some(success),
                        capture_denials_error: Some(CaptureDenialsErrorOutput {
                            message: message.clone(),
                            etl_path: String::new(),
                        }),
                    }),
                    result: Err(message),
                }
            }
        },
        Err(write_error) => {
            // The JSON write failed. When the ETL was successfully retained,
            // preserve its path so the caller can still find/delete it.
            let retained_etl = match retention {
                Some((destination, Ok(()))) => Some(destination.to_string_lossy().into_owned()),
                _ => None,
            };
            let message = write_error.to_string();
            let metadata = retained_etl.map(|etl_path| SandboxOutputMetadata {
                capture_denials: None,
                capture_denials_error: Some(CaptureDenialsErrorOutput {
                    message: message.clone(),
                    etl_path,
                }),
            });
            GuardedCaptureFinalization {
                metadata,
                result: Err(message),
            }
        }
    }
}
