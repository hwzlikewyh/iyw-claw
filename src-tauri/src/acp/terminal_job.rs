use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use tokio::process::{Child, Command};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
};
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};

#[link(name = "ntdll")]
extern "system" {
    fn NtResumeProcess(process: windows_sys::Win32::Foundation::HANDLE) -> i32;
}

/// 仅拥有本次 terminal 的进程树；正常退出不推断后代任务已完成。
pub(super) struct TerminalJob(OwnedHandle);

impl TerminalJob {
    fn new() -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(unsafe { OwnedHandle::from_raw_handle(handle) });
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        // 保留命令显式创建独立服务的能力；不把 breakaway 服务冒认为可回收后代。
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        let configured = unsafe {
            SetInformationJobObject(
                job.0.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if configured == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    pub(super) fn spawn(command: &mut Command) -> io::Result<(Child, Option<Self>)> {
        let job = match Self::new() {
            Ok(job) => job,
            Err(error) => {
                tracing::warn!(%error, "[ACP] terminal Job unavailable; retaining direct-child control");
                return command
                    .kill_on_drop(true)
                    .spawn()
                    .map(|child| (child, None));
            }
        };
        // 在执行任何用户代码前归入 Job，避免 spawn 后再绑定漏掉快速派生的后代。
        command
            .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        let handle = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("terminal handle missing"))?;
        let assigned = unsafe { AssignProcessToJobObject(job.0.as_raw_handle(), handle) } != 0;
        if !assigned {
            tracing::warn!(error = %io::Error::last_os_error(), "[ACP] terminal Job assignment unavailable");
        }
        let resume_status = unsafe { NtResumeProcess(handle) };
        if resume_status < 0 {
            let _ = child.start_kill();
            return Err(io::Error::other(format!(
                "failed to resume terminal process: NTSTATUS {resume_status:#x}"
            )));
        }
        Ok((child, assigned.then_some(job)))
    }

    pub(super) fn terminate(&self) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(super) fn has_processes(&self) -> io::Result<bool> {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        let result = unsafe {
            QueryInformationJobObject(
                self.0.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                std::ptr::addr_of_mut!(info).cast(),
                std::mem::size_of_val(&info) as u32,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(info.ActiveProcesses > 0)
    }
}
