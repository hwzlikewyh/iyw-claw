use std::ffi::OsStr;

pub(crate) const SETUP_FLAG: &str = "--internal-xinghe-sandbox-setup";
pub(crate) const RUNNER_FLAG: &str = "--internal-xinghe-command-runner";

pub(crate) fn setup_args() -> impl Iterator<Item = &'static str> {
    cfg!(feature = "bundled-host")
        .then_some(SETUP_FLAG)
        .into_iter()
}

#[cfg(feature = "bundled-host")]
pub fn dispatch_embedded_role() -> bool {
    let first = std::env::args_os().nth(1);
    let result = match first.as_deref() {
        Some(flag) if flag == OsStr::new(SETUP_FLAG) => crate::setup_helper_main(),
        Some(flag) if flag == OsStr::new(RUNNER_FLAG) => crate::bundled_runner::main(),
        _ => return false,
    };
    let status = match result {
        Ok(()) => 0,
        Err(_) => {
            // 详细错误写入原沙箱日志，避免输出 setup payload 或敏感路径。
            eprintln!("Xinghe sandbox role failed; see sandbox log");
            1
        }
    };
    std::process::exit(status);
}

#[cfg(feature = "bundled-host")]
pub fn elevate_desktop() -> std::io::Result<bool> {
    use windows_sys::Win32::UI::Shell::{
        IsUserAnAdmin, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    if unsafe { IsUserAnAdmin() } != 0 {
        return Ok(false);
    }
    let executable = crate::winutil::to_wide(std::env::current_exe()?);
    let arguments = std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string().map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "desktop argument is not UTF-8",
                )
            })
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    let parameters = crate::winutil::to_wide(crate::winutil::argv_to_command_line(&arguments));
    let directory = crate::winutil::to_wide(std::env::current_dir()?);
    let verb = crate::winutil::to_wide("runas");
    let mut request: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    request.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    request.fMask = SEE_MASK_NOASYNC;
    request.lpVerb = verb.as_ptr();
    request.lpFile = executable.as_ptr();
    request.lpParameters = parameters.as_ptr();
    request.lpDirectory = directory.as_ptr();
    request.nShow = 1;
    if unsafe { ShellExecuteExW(&mut request) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(true)
}
