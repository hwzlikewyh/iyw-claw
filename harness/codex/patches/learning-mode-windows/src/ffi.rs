// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Windows runtime FFI for the `processmodel.dll` Learning Mode trace exports.
//!
//! The three official V2 exports are resolved via
//! `LoadLibraryExW(LOAD_LIBRARY_SEARCH_SYSTEM32)` and `GetProcAddress`.
//! `processmodel.dll` is intentionally never freed: it is a system DLL that
//! stays resident for the process lifetime, so the module handle is used only to
//! resolve exports and then dropped without `FreeLibrary`.

use std::path::Path;
use std::ptr;
use std::sync::OnceLock;
use std::time::Duration;

use windows::Win32::Foundation::{
    GetLastError, ERROR_BUSY, ERROR_LOCK_VIOLATION, ERROR_RETRY, ERROR_SHARING_VIOLATION, HANDLE,
    HMODULE,
};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows_core::{HRESULT, PCSTR, PCWSTR};
use wxc_common::string_util;

use crate::LearningModeError;

/// System DLL that hosts the flat Learning Mode trace exports.
const PROCESSMODEL_DLL: &str = "processmodel.dll";

/// `HRESULT StartLearningModeTrace(HANDLE securityEnvironment, HLEARNINGMODE_TRACE* trace)`.
///
/// `HLEARNINGMODE_TRACE` is a `typedef HANDLE`; the export surfaces it through the
/// out-parameter.
type PfnStartLearningModeTrace = unsafe extern "system" fn(
    process_security_environment: HANDLE,
    trace_out: *mut HANDLE,
) -> HRESULT;

/// `HRESULT StopLearningModeTrace(HLEARNINGMODE_TRACE trace, LPCWSTR outputEtlPath)`.
///
/// A non-null `output_path` names a file the export opens under the caller's own
/// identity; the broker seals and copies the ETL into it. A null `output_path`
/// stops without delivery. The handle remains valid so the caller may retry
/// delivery until it closes the trace.
type PfnStopLearningModeTrace =
    unsafe extern "system" fn(trace: HANDLE, output_path: *const u16) -> HRESULT;

/// `void CloseLearningModeTrace(HLEARNINGMODE_TRACE trace)`.
type PfnCloseLearningModeTrace = unsafe extern "system" fn(trace: HANDLE);

/// Opaque handle to an in-progress Learning Mode trace (`HLEARNINGMODE_TRACE`).
///
/// Obtained from [`LearningModeApi::start_trace`]. [`LearningModeApi::stop_trace`]
/// borrows it so delivery can be retried. Dropping or explicitly closing the
/// handle releases all broker state; closing without stopping discards the trace.
pub struct LearningModeTraceHandle {
    raw: HANDLE,
    close: PfnCloseLearningModeTrace,
}

impl LearningModeTraceHandle {
    fn new(raw: HANDLE, close: PfnCloseLearningModeTrace) -> Self {
        Self { raw, close }
    }

    /// Close the trace and release all service-managed state.
    pub fn close(mut self) {
        self.close_inner();
    }

    fn close_inner(&mut self) {
        if !self.raw.0.is_null() {
            // SAFETY: `raw` was returned by `StartLearningModeTrace`, and
            // `close` was resolved from the same processmodel.dll contract.
            unsafe { (self.close)(self.raw) };
            self.raw = HANDLE(ptr::null_mut());
        }
    }
}

impl std::fmt::Debug for LearningModeTraceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("LearningModeTraceHandle")
            .field(&self.raw)
            .finish()
    }
}

impl Drop for LearningModeTraceHandle {
    fn drop(&mut self) {
        self.close_inner();
    }
}

/// Resolved Learning Mode trace exports from `processmodel.dll`.
///
/// Construct with [`LearningModeApi::load`]. Cloning is cheap (the struct holds three
/// function pointers into the resident system DLL).
#[derive(Clone, Copy)]
pub struct LearningModeApi {
    start: PfnStartLearningModeTrace,
    stop: PfnStopLearningModeTrace,
    close: PfnCloseLearningModeTrace,
}

impl std::fmt::Debug for LearningModeApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LearningModeApi")
            .field("start", &(self.start as *const ()))
            .field("stop", &(self.stop as *const ()))
            .field("close", &(self.close as *const ()))
            .finish()
    }
}

impl LearningModeApi {
    const STOP_DELIVERY_ATTEMPTS: usize = 3;
    const STOP_RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(25), Duration::from_millis(75)];

    /// Load `processmodel.dll` and resolve the Learning Mode trace exports.
    ///
    /// The result — success or failure — is memoized for the lifetime of the
    /// process: `processmodel.dll` is a resident system DLL whose export set does
    /// not change while the process runs, so repeated probes would only repeat the
    /// same `LoadLibraryExW`/`GetProcAddress` work and return the same answer. The
    /// cached error is cloned (see [`LearningModeError`]), preserving the original
    /// diagnostic on every call.
    ///
    /// # Errors
    /// - [`LearningModeError::DllLoad`] if `processmodel.dll` cannot be loaded.
    /// - [`LearningModeError::ExportMissing`] if any export is absent. Requiring
    ///   `CloseLearningModeTrace` rejects builds that expose the incompatible
    ///   earlier two-export ABI.
    pub fn load() -> Result<Self, LearningModeError> {
        static CACHE: OnceLock<Result<LearningModeApi, LearningModeError>> = OnceLock::new();
        CACHE.get_or_init(Self::load_uncached).clone()
    }

    /// Perform the actual DLL load and export resolution, bypassing the cache.
    fn load_uncached() -> Result<Self, LearningModeError> {
        let dll = string_util::to_wide(PROCESSMODEL_DLL);

        // SAFETY: `dll` is a valid null-terminated wide string that outlives the call.
        // `LOAD_LIBRARY_SEARCH_SYSTEM32` restricts the search to System32, preventing
        // DLL-planting. The module handle is used only for `GetProcAddress` below and
        // is never freed (the DLL stays resident for the process lifetime). Each
        // resolved pointer is transmuted to a signature that matches the C
        // declaration of the corresponding export exactly.
        unsafe {
            let hmodule = LoadLibraryExW(PCWSTR(dll.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32)
                .map_err(|e| LearningModeError::DllLoad(e.to_string()))?;

            let start_proc = resolve_export(hmodule, START_NAME)?;
            let stop_proc = resolve_export(hmodule, STOP_NAME)?;
            let close_proc = resolve_export(hmodule, CLOSE_NAME)?;

            let start: PfnStartLearningModeTrace = std::mem::transmute(start_proc);
            let stop: PfnStopLearningModeTrace = std::mem::transmute(stop_proc);
            let close: PfnCloseLearningModeTrace = std::mem::transmute(close_proc);

            Ok(Self { start, stop, close })
        }
    }

    /// Start a Learning Mode trace for the sandbox identified by
    /// `security_environment`.
    ///
    /// # Safety
    /// `security_environment` must be a live `HPROCESS_SECURITY_ENVIRONMENT` handle
    /// obtained from the sandbox launch path; the broker resolves it to the target
    /// AppContainer SID server-side.
    ///
    /// # Errors
    /// [`LearningModeError::HResultCall`] if the export returns a failing HRESULT.
    pub unsafe fn start_trace(
        &self,
        security_environment: HANDLE,
    ) -> Result<LearningModeTraceHandle, LearningModeError> {
        let mut trace = HANDLE(ptr::null_mut());
        // SAFETY: `self.start` was resolved from `processmodel.dll` and matches the
        // declared C signature; `trace` is a valid out-pointer. The caller upholds
        // the validity of `security_environment` per this method's safety contract.
        let result = (self.start)(security_environment, &mut trace);
        if result.is_err() {
            return Err(LearningModeError::HResultCall {
                function: "StartLearningModeTrace",
                code: result.0,
            });
        }
        if trace.0.is_null() {
            return Err(LearningModeError::HResultCall {
                function: "StartLearningModeTrace",
                code: windows::Win32::Foundation::E_UNEXPECTED.0,
            });
        }
        Ok(LearningModeTraceHandle::new(trace, self.close))
    }

    /// Stop `trace`, sealing and copying the ETL into `output_path`. Passing `None`
    /// stops without delivery.
    ///
    /// The handle remains live after success or failure, so callers may retry with
    /// the same or a different output path before closing it.
    ///
    /// # Errors
    /// - [`LearningModeError::InvalidInput`] if `output_path` contains an embedded NUL.
    /// - [`LearningModeError::HResultCall`] if the export returns a failing HRESULT.
    pub fn stop_trace(
        &self,
        trace: &LearningModeTraceHandle,
        output_path: Option<&Path>,
    ) -> Result<(), LearningModeError> {
        let wide_path = encode_output_path(output_path)?;
        self.stop_trace_encoded(trace, wide_path.as_deref())
    }

    /// Stop and deliver the trace, retrying only transient output-delivery
    /// failures. The trace remains live throughout the attempts and is still
    /// owned by the caller when this method returns.
    pub(crate) fn stop_trace_with_retry(
        &self,
        trace: &LearningModeTraceHandle,
        output_path: Option<&Path>,
    ) -> Result<(), LearningModeError> {
        for attempt in 0..Self::STOP_DELIVERY_ATTEMPTS {
            match self.stop_trace(trace, output_path) {
                Err(error)
                    if attempt + 1 < Self::STOP_DELIVERY_ATTEMPTS
                        && is_retryable_stop_error(&error) =>
                {
                    std::thread::sleep(Self::STOP_RETRY_DELAYS[attempt]);
                }
                result => return result,
            }
        }
        unreachable!("STOP_DELIVERY_ATTEMPTS is non-zero")
    }

    fn stop_trace_encoded(
        &self,
        trace: &LearningModeTraceHandle,
        wide_path: Option<&[u16]>,
    ) -> Result<(), LearningModeError> {
        let path_ptr = wide_path.map_or(ptr::null(), |path| path.as_ptr());

        // SAFETY: `self.stop` was resolved from `processmodel.dll` and matches the
        // declared C signature. `trace.raw` came from a prior `start_trace`, and
        // `path_ptr` is either null or points at the null-terminated `wide_path`
        // buffer, which outlives the call.
        let result = unsafe { (self.stop)(trace.raw, path_ptr) };
        if result.is_err() {
            return Err(LearningModeError::HResultCall {
                function: "StopLearningModeTrace",
                code: result.0,
            });
        }
        Ok(())
    }
}

fn is_retryable_stop_error(error: &LearningModeError) -> bool {
    let LearningModeError::HResultCall {
        function: "StopLearningModeTrace",
        code,
    } = error
    else {
        return false;
    };

    [
        ERROR_SHARING_VIOLATION,
        ERROR_LOCK_VIOLATION,
        ERROR_BUSY,
        ERROR_RETRY,
    ]
    .into_iter()
    .any(|win32| *code == HRESULT::from_win32(win32.0).0)
}

fn encode_output_path(output_path: Option<&Path>) -> Result<Option<Vec<u16>>, LearningModeError> {
    output_path
        .map(|path| {
            string_util::os_str_to_wide(path.as_os_str()).map_err(|_| {
                LearningModeError::InvalidInput {
                    parameter: "output_path",
                    detail: "path contains an embedded NUL".to_string(),
                }
            })
        })
        .transpose()
}

/// Resolve a single export from an already-loaded module, mapping a missing symbol
/// to [`LearningModeError::ExportMissing`].
///
/// # Safety
/// `hmodule` must be a valid module handle.
unsafe fn resolve_export(
    hmodule: HMODULE,
    name: &'static std::ffi::CStr,
) -> Result<unsafe extern "system" fn() -> isize, LearningModeError> {
    // SAFETY: `name` is a valid null-terminated C string; `hmodule` is valid per the
    // caller's contract.
    match GetProcAddress(hmodule, PCSTR(name.as_ptr().cast())) {
        Some(proc) => Ok(proc),
        None => Err(LearningModeError::ExportMissing {
            api: "Learning Mode trace",
            export: name.to_str().unwrap_or("<non-utf8 export>"),
            detail: format!(
                "GetProcAddress returned NULL (GetLastError = {})",
                last_error()
            ),
        }),
    }
}

/// Capture `GetLastError` as a plain `u32`.
fn last_error() -> u32 {
    // SAFETY: `GetLastError` has no preconditions and no side effects beyond reading
    // the calling thread's last-error slot.
    unsafe { GetLastError().0 }
}

/// Undecorated names of the three Learning Mode trace exports, in the order the
/// 2-phase capture lifecycle uses them.
const START_NAME: &core::ffi::CStr = c"StartLearningModeTrace";
const STOP_NAME: &core::ffi::CStr = c"StopLearningModeTrace";
const CLOSE_NAME: &core::ffi::CStr = c"CloseLearningModeTrace";

/// Which Learning Mode trace exports resolved on this machine.
///
/// This mirrors the security-environment report shape and isolates the pure
/// all-or-nothing completeness rule so it can be unit-tested without a live DLL.
/// Requiring `close` in addition to `start`/`stop` is what rejects the incompatible
/// earlier two-export ("V1") ABI.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LearningModeExportReport {
    /// Resolved name of `StartLearningModeTrace`, if present.
    pub start: Option<&'static str>,
    /// Resolved name of `StopLearningModeTrace`, if present.
    pub stop: Option<&'static str>,
    /// Resolved name of `CloseLearningModeTrace`, if present.
    pub close: Option<&'static str>,
}

impl LearningModeExportReport {
    /// `true` only when all three trace exports resolved. A start+stop-only build
    /// (the legacy two-export ABI) is deliberately incomplete.
    pub(crate) fn is_complete(&self) -> bool {
        self.start.is_some() && self.stop.is_some() && self.close.is_some()
    }
}

/// Probe `processmodel.dll` for the three Learning Mode trace exports. Returns an
/// all-`None` report if the DLL itself cannot be loaded.
fn probe_learning_mode_exports() -> LearningModeExportReport {
    let dll = string_util::to_wide(PROCESSMODEL_DLL);
    // SAFETY: `dll` is a valid null-terminated wide string that outlives the call;
    // `LOAD_LIBRARY_SEARCH_SYSTEM32` restricts the search to System32.
    let hmodule =
        match unsafe { LoadLibraryExW(PCWSTR(dll.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32) } {
            Ok(h) => h,
            Err(_) => return LearningModeExportReport::default(),
        };

    // SAFETY: `hmodule` is valid; `export_name_if_present` only reads exports.
    unsafe {
        LearningModeExportReport {
            start: export_name_if_present(hmodule, START_NAME),
            stop: export_name_if_present(hmodule, STOP_NAME),
            close: export_name_if_present(hmodule, CLOSE_NAME),
        }
    }
}

/// Return `name` if it resolves in `hmodule`, otherwise `None`.
///
/// # Safety
/// `hmodule` must be a valid module handle.
unsafe fn export_name_if_present(
    hmodule: HMODULE,
    name: &'static core::ffi::CStr,
) -> Option<&'static str> {
    // SAFETY: `name` is a valid null-terminated C string; `hmodule` is valid.
    if unsafe { GetProcAddress(hmodule, PCSTR(name.as_ptr().cast())) }.is_some() {
        name.to_str().ok()
    } else {
        None
    }
}

/// Capability probe: `true` only when `processmodel.dll` exposes all three Learning Mode
/// trace exports on this machine.
#[must_use]
pub fn is_learning_mode_api_available() -> bool {
    probe_learning_mode_exports().is_complete()
}
