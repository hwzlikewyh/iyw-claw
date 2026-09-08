// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! RAII wrapper around a Windows Job Object used to apply UI restrictions
//! (`JOB_OBJECT_UILIMIT_*`) to a child process and any descendants it creates,
//! plus the Windows-specific encoder that maps a platform-agnostic
//! [`wxc_common::ui_policy::EffectiveUiRestrictions`] to the corresponding bitmask.
//!
//! The wrapper owns the underlying job HANDLE and closes it on drop. Jobs are
//! configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so an abandoned
//! sandbox cannot outlive the process that owns its enforcement state.

use core::ffi::c_void;
use std::mem::size_of;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
    JobObjectBasicUIRestrictions, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_BASIC_UI_RESTRICTIONS, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_UILIMIT, JOB_OBJECT_UILIMIT_DESKTOP,
    JOB_OBJECT_UILIMIT_DISPLAYSETTINGS, JOB_OBJECT_UILIMIT_EXITWINDOWS,
    JOB_OBJECT_UILIMIT_GLOBALATOMS, JOB_OBJECT_UILIMIT_HANDLES, JOB_OBJECT_UILIMIT_READCLIPBOARD,
    JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS, JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
};
use windows::Win32::System::SystemServices::JOB_OBJECT_UILIMIT_IME;
use windows_core::PCWSTR;

use wxc_common::error::WxcError;
use wxc_common::ui_policy::EffectiveUiRestrictions;

/// Helper for loading `RtlGetVersion` from `ntdll.dll` to get the true
/// (unshimmed) OS version. `GetVersionExW` lies on post-8.1 builds due
/// to the compatibility shim.
mod version_detect {
    use std::mem::size_of;

    use windows::Win32::Foundation::NTSTATUS;
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;

    type RtlGetVersionFn = unsafe extern "system" fn(version_info: *mut OSVERSIONINFOW) -> NTSTATUS;

    /// Returns the real OS build number by calling `RtlGetVersion` from
    /// `ntdll.dll`. Falls back to `u32::MAX` if the symbol cannot be
    /// resolved or the call fails. `u32::MAX` is deliberately treated as
    /// "modern" by capability gating so an indeterminate probe fails secure
    /// (the more restrictive flag is kept rather than silently dropped).
    pub(super) fn get_os_build_number() -> u32 {
        // SAFETY: ntdll.dll is always loaded in every Windows process.
        // `GetModuleHandleW` with "ntdll.dll" returns the existing module
        // handle without incrementing a reference count.
        unsafe {
            let module = windows::Win32::System::LibraryLoader::GetModuleHandleW(
                windows::core::w!("ntdll.dll"),
            );
            let module = match module {
                Ok(h) => h,
                Err(_) => return u32::MAX,
            };
            let proc = windows::Win32::System::LibraryLoader::GetProcAddress(
                module,
                windows::core::s!("RtlGetVersion"),
            );
            let proc = match proc {
                Some(p) => p,
                None => return u32::MAX,
            };
            let rtl_get_version: RtlGetVersionFn = std::mem::transmute(proc);
            let mut info = OSVERSIONINFOW {
                dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
                ..Default::default()
            };
            let status = rtl_get_version(&mut info);
            if status.is_ok() {
                info.dwBuildNumber
            } else {
                u32::MAX
            }
        }
    }
}

/// `JOB_OBJECT_UILIMIT_INJECTION` from `winnt.h`. The `windows` crate
/// does not emit this constant; if a future release adds it, the local
/// definition can be removed and the import above extended.
const JOB_OBJECT_UILIMIT_INJECTION: u32 = 0x0000_0200;

/// Every `JOB_OBJECT_UILIMIT_*` bit this module's encoder
/// ([`to_job_object_uilimit_mask`]) can emit. Acts as the universe for the
/// capability intersection performed by [`supported_ui_limit_mask`]. Must
/// stay in sync with the encoder — the `encoder_known_bit_positions` test
/// pins the all-restrictions mask to this value.
const ALL_DEFINED_UI_LIMITS: u32 = 0x0000_03FF;

/// Minimum OS build that supports `JOB_OBJECT_UILIMIT_IME` (0x100).
/// This flag is empirically accepted on Windows 11 22H2 (22621) and later
/// — confirmed on 22631 (23H2) — but rejected with `ERROR_INVALID_PARAMETER`
/// on Windows Server 2022 (20348). Its exact introduction build between 20348
/// and 22631 is unconfirmed, so it is gated at the 22H2 boundary: builds below
/// it conservatively omit the flag (UI-limit support is monotonic — once a
/// build accepts the flag, every later build does too — so this never hands
/// the kernel a flag it would reject).
const MIN_BUILD_FOR_IME_LIMIT: u32 = 22621;

/// Minimum OS build that supports `JOB_OBJECT_UILIMIT_INJECTION` (0x200).
/// Windows 11 26100 (24H2) introduced this flag; earlier builds reject it
/// with `ERROR_INVALID_PARAMETER`, so it is excluded from the supported
/// UI-limit set on those builds.
const MIN_BUILD_FOR_INJECTION_LIMIT: u32 = 26100;
const JOB_EMPTY_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const JOB_EMPTY_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Cached OS build number (queried once via `RtlGetVersion`).
static OS_BUILD_NUMBER: OnceLock<u32> = OnceLock::new();

/// Returns the current OS build number, caching the result for the process
/// lifetime. Returns `u32::MAX` when the build cannot be determined, which
/// capability gating treats as "modern" so detection failures fail secure.
pub fn os_build_number() -> u32 {
    *OS_BUILD_NUMBER.get_or_init(version_detect::get_os_build_number)
}

/// Returns `true` when the current OS build can enforce
/// `JOB_OBJECT_UILIMIT_INJECTION` (input-injection blocking). Introduced in
/// build 26100; an unknown build is reported as supported (fail secure).
pub fn input_injection_blocking_supported() -> bool {
    os_build_number() >= MIN_BUILD_FOR_INJECTION_LIMIT
}

/// Pure capability map: given an OS build number, returns the subset of
/// encoder-defined `JOB_OBJECT_UILIMIT_*` flags the kernel can enforce on
/// that build. `JOB_OBJECT_UILIMIT_IME` and `JOB_OBJECT_UILIMIT_INJECTION`
/// are build-gated; all other flags are universally supported.
fn supported_ui_limit_mask_for_build(build: u32) -> u32 {
    let mut supported = ALL_DEFINED_UI_LIMITS;
    if build < MIN_BUILD_FOR_IME_LIMIT {
        supported &= !JOB_OBJECT_UILIMIT_IME;
    }
    if build < MIN_BUILD_FOR_INJECTION_LIMIT {
        supported &= !JOB_OBJECT_UILIMIT_INJECTION;
    }
    supported
}

#[inline(always)]
fn has_ui_limit(mask: u32, flag: u32) -> bool {
    (mask & flag) == flag
}

fn supported_ui_restrictions_for_build(build: u32) -> EffectiveUiRestrictions {
    let supported = supported_ui_limit_mask_for_build(build);
    EffectiveUiRestrictions {
        block_clipboard_read: has_ui_limit(supported, JOB_OBJECT_UILIMIT_READCLIPBOARD.0),
        block_clipboard_write: has_ui_limit(supported, JOB_OBJECT_UILIMIT_WRITECLIPBOARD.0),
        block_input_injection: has_ui_limit(supported, JOB_OBJECT_UILIMIT_INJECTION),
        block_input_method_changes: has_ui_limit(supported, JOB_OBJECT_UILIMIT_IME),
        block_external_ui_objects: has_ui_limit(supported, JOB_OBJECT_UILIMIT_HANDLES.0),
        block_global_ui_namespace: has_ui_limit(supported, JOB_OBJECT_UILIMIT_GLOBALATOMS.0),
        block_desktop_switching: has_ui_limit(supported, JOB_OBJECT_UILIMIT_DESKTOP.0),
        block_logoff_or_shutdown: has_ui_limit(supported, JOB_OBJECT_UILIMIT_EXITWINDOWS.0),
        block_system_parameter_changes: has_ui_limit(
            supported,
            JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS.0,
        ),
        block_display_settings_changes: has_ui_limit(
            supported,
            JOB_OBJECT_UILIMIT_DISPLAYSETTINGS.0,
        ),
    }
}

/// Returns the subset of encoder-defined `JOB_OBJECT_UILIMIT_*` flags the
/// current OS build can enforce. The effective restriction mask applied to a
/// job is always `requested & supported`, so the kernel is never handed a
/// flag it would reject.
pub fn supported_ui_limit_mask() -> u32 {
    supported_ui_limit_mask_for_build(os_build_number())
}

/// Returns the platform-agnostic UI restrictions the current OS build can
/// enforce. Reported to callers via `wxc-exec --probe`.
pub fn supported_ui_restrictions() -> EffectiveUiRestrictions {
    supported_ui_restrictions_for_build(os_build_number())
}

/// Encode platform-agnostic UI restrictions as the `JOB_OBJECT_UILIMIT_*`
/// bitmask consumed by `SetInformationJobObject(JobObjectBasicUIRestrictions)`
/// and by the BaseContainer SandboxSpec `ui_restrictions` field.
pub fn to_job_object_uilimit_mask(r: &EffectiveUiRestrictions) -> u32 {
    let mut mask: u32 = 0;
    if r.block_external_ui_objects {
        mask |= JOB_OBJECT_UILIMIT_HANDLES.0;
    }
    if r.block_clipboard_read {
        mask |= JOB_OBJECT_UILIMIT_READCLIPBOARD.0;
    }
    if r.block_clipboard_write {
        mask |= JOB_OBJECT_UILIMIT_WRITECLIPBOARD.0;
    }
    if r.block_system_parameter_changes {
        mask |= JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS.0;
    }
    if r.block_display_settings_changes {
        mask |= JOB_OBJECT_UILIMIT_DISPLAYSETTINGS.0;
    }
    if r.block_global_ui_namespace {
        mask |= JOB_OBJECT_UILIMIT_GLOBALATOMS.0;
    }
    if r.block_desktop_switching {
        mask |= JOB_OBJECT_UILIMIT_DESKTOP.0;
    }
    if r.block_logoff_or_shutdown {
        mask |= JOB_OBJECT_UILIMIT_EXITWINDOWS.0;
    }
    if r.block_input_method_changes {
        mask |= JOB_OBJECT_UILIMIT_IME;
    }
    if r.block_input_injection {
        mask |= JOB_OBJECT_UILIMIT_INJECTION;
    }
    mask
}

/// RAII wrapper for an unnamed Windows Job Object configured for UI
/// restrictions. The job HANDLE is closed when this value is dropped.
pub struct UiJobObject {
    handle: HANDLE,
}

impl UiJobObject {
    /// Creates an unnamed Job Object owned by the current process.
    pub fn new() -> Result<Self, WxcError> {
        // SAFETY: CreateJobObjectW with NULL security attributes and NULL name
        // is documented to either return a valid HANDLE or an error.
        let handle = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
            .map_err(|e| WxcError::Process(format!("CreateJobObjectW: {e}")))?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Err(error) = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(WxcError::Process(format!(
                "SetInformationJobObject(KILL_ON_JOB_CLOSE): {error}"
            )));
        }
        Ok(Self { handle })
    }

    /// Applies the given UI restrictions via `JobObjectBasicUIRestrictions`.
    /// Passing `EffectiveUiRestrictions::default()` clears all UI restrictions
    /// and is a valid no-op call.
    ///
    /// The mask actually applied is `requested & supported_ui_limit_mask()`:
    /// flags the current OS build cannot enforce (e.g.
    /// `JOB_OBJECT_UILIMIT_INJECTION` on builds older than 26100) are dropped
    /// so the call never fails with `ERROR_INVALID_PARAMETER`. Which flags a
    /// host can enforce is reported by `wxc-exec --probe`.
    pub fn set_ui_limits(&self, restrictions: &EffectiveUiRestrictions) -> Result<(), WxcError> {
        let mask = to_job_object_uilimit_mask(restrictions) & supported_ui_limit_mask();

        let info = JOBOBJECT_BASIC_UI_RESTRICTIONS {
            UIRestrictionsClass: JOB_OBJECT_UILIMIT(mask),
        };
        // SAFETY: `info` is a valid, fully-initialized struct living on the
        // stack for the duration of the call. The size matches the struct
        // type that JobObjectBasicUIRestrictions expects.
        unsafe {
            SetInformationJobObject(
                self.handle,
                JobObjectBasicUIRestrictions,
                &info as *const _ as *const c_void,
                size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>() as u32,
            )
        }
        .map_err(|e| WxcError::Process(format!("SetInformationJobObject(UI): {e}")))
    }

    /// Assigns the given process handle to this job. The process and any
    /// future descendants will inherit the job's UI restrictions.
    pub fn assign_process(&self, process_handle: HANDLE) -> Result<(), WxcError> {
        // SAFETY: Both handles must be valid for the duration of the call;
        // this is the caller's responsibility for `process_handle`.
        unsafe { AssignProcessToJobObject(self.handle, process_handle) }
            .map_err(|e| WxcError::Process(format!("AssignProcessToJobObject: {e}")))
    }

    /// Returns this process's numeric HANDLE value for authenticated
    /// cross-process duplication by the elevated guarded-WPR guardian.
    pub fn handle_value(&self) -> usize {
        self.handle.0 as usize
    }

    /// Terminates every process currently assigned to this job (the sandboxed
    /// child and all of its descendants) with the given exit code.
    pub fn terminate(&self, exit_code: u32) -> Result<(), WxcError> {
        // SAFETY: `self.handle` is a valid job handle owned by this struct.
        unsafe { TerminateJobObject(self.handle, exit_code) }
            .map_err(|error| WxcError::Process(format!("TerminateJobObject: {error}")))
    }

    /// Waits until no processes remain assigned to the job.
    pub fn wait_for_empty(&self) -> Result<(), WxcError> {
        self.wait_for_empty_within(JOB_EMPTY_WAIT_TIMEOUT, JOB_EMPTY_POLL_INTERVAL)
    }

    /// Waits up to `timeout` for the job to reach zero active processes, polling
    /// job accounting every `poll_interval`. Extracted from [`Self::wait_for_empty`]
    /// so callers (and tests) can supply an explicit bound; `Duration::ZERO`
    /// performs exactly one accounting probe with no sleep. The timeout message
    /// reports the configured `timeout`.
    fn wait_for_empty_within(
        &self,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<(), WxcError> {
        let deadline = Instant::now() + timeout;
        loop {
            let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            // SAFETY: `self.handle` is a valid job handle and `accounting`
            // matches the requested information class and buffer length.
            unsafe {
                QueryInformationJobObject(
                    Some(self.handle),
                    JobObjectBasicAccountingInformation,
                    &mut accounting as *mut _ as *mut c_void,
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    None,
                )
            }
            .map_err(|error| {
                WxcError::Process(format!(
                    "QueryInformationJobObject(BasicAccounting): {error}"
                ))
            })?;
            if accounting.ActiveProcesses == 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(WxcError::Process(format!(
                    "timed out after {}ms waiting for sandbox job to become empty; {} process(es) \
                     remain active",
                    timeout.as_millis(),
                    accounting.ActiveProcesses
                )));
            }
            std::thread::sleep(poll_interval);
        }
    }

    /// Terminates the complete process tree and confirms that the job is empty.
    ///
    /// This is the **strict** drain: a failure to observe the job reach zero
    /// active processes within [`JOB_EMPTY_WAIT_TIMEOUT`] is returned as an
    /// error. Use it only where full drain certainty is a correctness
    /// requirement — notably the guarded-WPR `captureDenials` paths, where ETL
    /// scoping is only sound if nothing can still be running unobserved.
    /// Ordinary (non-capture) teardown should prefer
    /// [`Self::terminate_best_effort`], which does not fail an otherwise-valid
    /// run just because the kernel had not finished tearing the tree down
    /// within the window.
    pub fn terminate_and_wait(&self, exit_code: u32) -> Result<(), WxcError> {
        self.terminate(exit_code)?;
        self.wait_for_empty()
    }

    /// Terminates the complete process tree, treating a drain-observation
    /// timeout as a recoverable warning rather than a hard failure.
    ///
    /// A failure of [`Self::terminate`] itself (the actual `TerminateJobObject`
    /// call) is still returned as an error. But if the job does not reach zero
    /// active processes within the window, this returns `Ok(Some(error))` so
    /// the caller can surface a warning while preserving the run's result — the
    /// kernel continues tearing the tree down, and
    /// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` guarantees eventual teardown when
    /// the handle closes. Returns `Ok(None)` when the job drained cleanly.
    pub fn terminate_best_effort(&self, exit_code: u32) -> Result<Option<WxcError>, WxcError> {
        self.terminate_best_effort_within(
            exit_code,
            JOB_EMPTY_WAIT_TIMEOUT,
            JOB_EMPTY_POLL_INTERVAL,
        )
    }

    /// [`Self::terminate_best_effort`] with an explicit drain bound. Factored
    /// out so the warning (drain-timeout) path is unit-testable with
    /// `Duration::ZERO` instead of the multi-second production window.
    fn terminate_best_effort_within(
        &self,
        exit_code: u32,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<Option<WxcError>, WxcError> {
        self.terminate(exit_code)?;
        Ok(self.wait_for_empty_within(timeout, poll_interval).err())
    }
}

impl Drop for UiJobObject {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            // SAFETY: `self.handle` was produced by CreateJobObjectW and has
            // not been closed elsewhere — `UiJobObject` owns it.
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }
}
