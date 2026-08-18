use std::ptr;
use std::time::Instant;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_ABANDONED, WAIT_FAILED,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

const STARTUP_GATE_NAME: &str = "Local\\app.iywclaw-startup-gate-v1";
const STARTUP_GATE_WAIT_MS: u32 = 60_000;

pub struct DesktopStartupGate {
    handle: HANDLE,
}

impl Drop for DesktopStartupGate {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.handle);
            CloseHandle(self.handle);
        }
    }
}

pub fn acquire() -> Result<DesktopStartupGate, String> {
    let name = wide_null(STARTUP_GATE_NAME);
    let started = Instant::now();
    let handle = unsafe { CreateMutexW(ptr::null(), true.into(), name.as_ptr()) };
    if handle.is_null() {
        return Err(last_error("create desktop startup gate"));
    }

    let contended = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if contended {
        wait_for_existing_owner(handle)?;
    }
    tracing::info!(
        target: "iyw_claw_startup",
        event = "startup_gate_acquired",
        contended,
        wait_ms = started.elapsed().as_millis() as u64,
        "desktop startup gate acquired"
    );
    Ok(DesktopStartupGate { handle })
}

fn wait_for_existing_owner(handle: HANDLE) -> Result<(), String> {
    let result = unsafe { WaitForSingleObject(handle, STARTUP_GATE_WAIT_MS) };
    if matches!(result, WAIT_OBJECT_0 | WAIT_ABANDONED) {
        return Ok(());
    }
    let wait_error = (result == WAIT_FAILED).then(|| unsafe { GetLastError() });
    unsafe {
        CloseHandle(handle);
    }
    match result {
        WAIT_TIMEOUT => Err(format!(
            "desktop startup gate timed out after {STARTUP_GATE_WAIT_MS}ms"
        )),
        WAIT_FAILED => Err(format!(
            "wait for desktop startup gate failed with Windows error {}",
            wait_error.unwrap_or_default()
        )),
        other => Err(format!("unexpected desktop startup gate result: {other}")),
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error(action: &str) -> String {
    let code = unsafe { GetLastError() };
    format!("{action} failed with Windows error {code}")
}
