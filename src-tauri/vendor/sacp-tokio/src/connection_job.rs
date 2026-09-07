use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use tokio::process::{Child, Command};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process: windows_sys::Win32::Foundation::HANDLE) -> i32;
}

pub(super) struct ConnectionJob(OwnedHandle);

impl ConnectionJob {
    fn new() -> io::Result<Self> {
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(unsafe { OwnedHandle::from_raw_handle(raw) });
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        if unsafe {
            SetInformationJobObject(
                raw,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    pub(super) fn spawn(command: &mut Command) -> io::Result<(Child, Option<Self>)> {
        let job = match Self::new() {
            Ok(job) => job,
            Err(error) => {
                eprintln!("[ACP] runtime process containment unavailable: {error}");
                return command.spawn().map(|child| (child, None));
            }
        };
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let mut child = command.spawn()?;
        let handle = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("runtime process handle unavailable"))?;
        let assigned = unsafe { AssignProcessToJobObject(job.0.as_raw_handle(), handle) } != 0;
        if !assigned {
            eprintln!(
                "[ACP] runtime process Job assignment failed: {}",
                io::Error::last_os_error()
            );
        }
        let resume_status = unsafe { NtResumeProcess(handle) };
        if resume_status < 0 {
            let _ = child.start_kill();
            return Err(io::Error::other(format!(
                "failed to resume runtime process: NTSTATUS {resume_status:#x}"
            )));
        }
        Ok((child, assigned.then_some(job)))
    }

    pub(super) fn terminate(&self) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
            let error = io::Error::last_os_error();
            eprintln!("[ACP] owned runtime process cleanup failed: {error}");
            return Err(error);
        }
        Ok(())
    }
}
