use std::process::Child;

use anyhow::Result;

#[cfg(windows)]
pub struct ProbeJob(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl ProbeJob {
    pub fn attach(child: &Child) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let guard = Self(job);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if configured == 0 || unsafe { AssignProcessToJobObject(job, child.as_raw_handle()) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(guard)
    }
}

#[cfg(windows)]
impl Drop for ProbeJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(not(windows))]
pub struct ProbeJob;

#[cfg(not(windows))]
impl ProbeJob {
    pub fn attach(_child: &Child) -> Result<Self> {
        Ok(Self)
    }
}
