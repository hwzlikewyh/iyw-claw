#![cfg(target_os = "windows")]

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
use windows_sys::Win32::Security::{
    DuplicateTokenEx, GetTokenInformation, SecurityImpersonation, TokenElevationType,
    TokenElevationTypeFull, TokenPrimary, TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_SESSIONID,
    TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessWithTokenW, GetCurrentProcess, GetExitCodeProcess, OpenProcess, OpenProcessToken,
    TerminateProcess, PROCESS_INFORMATION, PROCESS_QUERY_INFORMATION, STARTUPINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};

use super::process::{kill_tree_checked, ProcessRecord};
use super::windows_process_values::{build_command_line, build_environment, to_wide};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;
const STILL_ACTIVE: u32 = 259;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnelevatedLaunchMode {
    Standard,
    Required,
}

pub(super) fn launch_mode() -> std::io::Result<UnelevatedLaunchMode> {
    if current_elevation_type()? != TokenElevationTypeFull {
        return Ok(UnelevatedLaunchMode::Standard);
    }
    if unsafe { GetShellWindow() }.is_null() {
        return Err(io_error("Explorer shell is unavailable"));
    }
    Ok(UnelevatedLaunchMode::Required)
}

fn current_elevation_type() -> std::io::Result<i32> {
    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(last_error("OpenProcessToken(current)"));
    }
    let guard = HandleGuard::new(token);
    let mut elevation_type = 0_i32;
    let mut returned = 0_u32;
    let ok = unsafe {
        GetTokenInformation(
            guard.raw(),
            TokenElevationType,
            &mut elevation_type as *mut i32 as *mut _,
            std::mem::size_of::<i32>() as u32,
            &mut returned,
        )
    };
    if ok == 0 {
        return Err(last_error("GetTokenInformation(TokenElevationType)"));
    }
    Ok(elevation_type)
}

pub(super) struct UnelevatedProcess {
    handle: HandleGuard,
    pid: u32,
}

impl UnelevatedProcess {
    pub(super) fn pid(&self) -> u32 {
        self.pid
    }

    pub(super) async fn wait(
        self,
        timeout: Duration,
        cancellation: CancellationToken,
        record: Option<&ProcessRecord>,
    ) -> std::io::Result<u32> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(code) = self.exit_code()? {
                return Ok(code);
            }
            if cancellation.is_cancelled() {
                self.stop(record).await;
                return Err(io_error("browser bootstrap was cancelled"));
            }
            if Instant::now() >= deadline {
                self.stop(record).await;
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "browser bootstrap timed out",
                ));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn exit_code(&self) -> std::io::Result<Option<u32>> {
        let mut code = 0_u32;
        if unsafe { GetExitCodeProcess(self.handle.raw(), &mut code) } == 0 {
            return Err(last_error("GetExitCodeProcess"));
        }
        Ok((code != STILL_ACTIVE).then_some(code))
    }

    async fn stop(&self, record: Option<&ProcessRecord>) {
        if let Some(record) = record {
            if kill_tree_checked(record).await.is_ok() {
                return;
            }
        }
        unsafe {
            TerminateProcess(self.handle.raw(), 1);
        }
    }
}

pub(super) fn spawn_unelevated(
    executable: &Path,
    args: &[OsString],
    overrides: &[(OsString, OsString)],
) -> std::io::Result<UnelevatedProcess> {
    let primary_token = explorer_primary_token()?;
    let application = to_wide(executable.as_os_str())?;
    let command_line = build_command_line(executable, args)?;
    let mut command_line = to_wide(OsStr::new(&command_line))?;
    let mut environment = build_environment(overrides)?;
    let current_dir = std::env::current_dir().ok();
    let current_dir = current_dir
        .as_ref()
        .map(|path| to_wide(path.as_os_str()))
        .transpose()?;
    create_process(
        primary_token.raw(),
        &application,
        &mut command_line,
        &mut environment,
        current_dir.as_deref(),
    )
}

fn explorer_primary_token() -> std::io::Result<HandleGuard> {
    let shell_window = unsafe { GetShellWindow() };
    if shell_window.is_null() {
        return Err(io_error("Explorer shell is unavailable"));
    }
    let mut shell_pid = 0_u32;
    unsafe { GetWindowThreadProcessId(shell_window, &mut shell_pid) };
    if shell_pid == 0 {
        return Err(last_error("GetWindowThreadProcessId(Explorer)"));
    }
    let shell = HandleGuard::new(unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, 0, shell_pid) });
    shell.ensure("OpenProcess(Explorer)")?;
    let mut shell_token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(shell.raw(), TOKEN_DUPLICATE | TOKEN_QUERY, &mut shell_token) }
        == 0
    {
        return Err(last_error("OpenProcessToken(Explorer)"));
    }
    duplicate_primary_token(HandleGuard::new(shell_token))
}

fn duplicate_primary_token(shell_token: HandleGuard) -> std::io::Result<HandleGuard> {
    let rights = TOKEN_QUERY
        | TOKEN_ASSIGN_PRIMARY
        | TOKEN_DUPLICATE
        | TOKEN_ADJUST_DEFAULT
        | TOKEN_ADJUST_SESSIONID;
    let mut primary_token: HANDLE = std::ptr::null_mut();
    let ok = unsafe {
        DuplicateTokenEx(
            shell_token.raw(),
            rights,
            std::ptr::null(),
            SecurityImpersonation,
            TokenPrimary,
            &mut primary_token,
        )
    };
    if ok == 0 {
        return Err(last_error("DuplicateTokenEx(Explorer)"));
    }
    Ok(HandleGuard::new(primary_token))
}

fn create_process(
    token: HANDLE,
    application: &[u16],
    command_line: &mut [u16],
    environment: &mut [u16],
    current_dir: Option<&[u16]>,
) -> std::io::Result<UnelevatedProcess> {
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let directory = current_dir.map_or(std::ptr::null(), |value| value.as_ptr());
    let ok = unsafe {
        CreateProcessWithTokenW(
            token,
            0,
            application.as_ptr(),
            command_line.as_mut_ptr(),
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            environment.as_mut_ptr().cast(),
            directory,
            &startup,
            &mut info,
        )
    };
    if ok == 0 {
        return Err(last_error("CreateProcessWithTokenW"));
    }
    unsafe { CloseHandle(info.hThread) };
    Ok(UnelevatedProcess {
        handle: HandleGuard::new(info.hProcess),
        pid: info.dwProcessId,
    })
}

fn last_error(context: &str) -> std::io::Error {
    io_error(format!("{context} failed with Win32 error {}", unsafe {
        GetLastError()
    }))
}

fn io_error(message: impl Into<String>) -> std::io::Error {
    std::io::Error::other(message.into())
}

struct HandleGuard(isize);

impl HandleGuard {
    fn new(handle: HANDLE) -> Self {
        Self(handle as isize)
    }

    fn raw(&self) -> HANDLE {
        self.0 as HANDLE
    }

    fn ensure(&self, context: &str) -> std::io::Result<()> {
        if self.0 == 0 {
            return Err(last_error(context));
        }
        Ok(())
    }
}

impl Drop for HandleGuard {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe { CloseHandle(self.raw()) };
            self.0 = 0;
        }
    }
}
