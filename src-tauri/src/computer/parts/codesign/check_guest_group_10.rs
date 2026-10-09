// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Check a running process against a designated-requirement string, and read
/// its signature if it passes.
///
/// `SecCodeCheckValidity` on a running process checks the kernel's view of it
/// (still validly signed) and that the code on disk is the code that is
/// running — a file replaced after `exec` fails here rather than passing on
/// the strength of the new file.
pub fn check_guest(guest: Guest, requirement: &str) -> Result<CodeInfo, String> {
    let code = copy_guest(guest)?;
    check_requirement(&code, requirement)?;
    signing_info(&code)
}

/// This process's own signature, unchecked. For the helper's interlock: a
/// helper that carries a Team ID but no compiled-in trust anchors refuses to
/// start.
pub fn self_info() -> Result<CodeInfo, String> {
    let mut code: SecCodeRef = std::ptr::null();
    // SAFETY: `code` receives a +1 reference on success.
    let status = unsafe { SecCodeCopySelf(K_SEC_CS_DEFAULT_FLAGS, &mut code) };
    if status != 0 {
        return Err(status_error("SecCodeCopySelf", status));
    }
    signing_info(&Owned(code))
}

/// The kernel's code-directory hash for the image `pid` is running, lowercase
/// hex. From the kernel, not the file: this is what is actually mapped.
pub fn running_cdhash(pid: u32) -> std::io::Result<String> {
    let pid = libc::pid_t::try_from(pid).map_err(std::io::Error::other)?;
    let mut hash = [0u8; 20];
    // SAFETY: a 20-byte buffer, and CS_OPS_CDHASH writes exactly 20.
    let rc = unsafe { csops(pid, CS_OPS_CDHASH, hash.as_mut_ptr().cast(), hash.len()) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(hex(&hash))
}

/// The kernel's code-signing status bits for `pid` (`CS_VALID`,
/// `CS_RUNTIME`, `CS_GET_TASK_ALLOW`, …).
pub fn running_status(pid: u32) -> std::io::Result<u32> {
    let pid = libc::pid_t::try_from(pid).map_err(std::io::Error::other)?;
    let mut flags: u32 = 0;
    // SAFETY: a u32 out-parameter, which is what CS_OPS_STATUS writes.
    let rc = unsafe {
        csops(
            pid,
            CS_OPS_STATUS,
            (&mut flags as *mut u32).cast(),
            std::mem::size_of::<u32>(),
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(flags)
}
