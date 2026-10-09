//! 查询当前 macOS 应用的辅助功能与屏幕录制权限，不弹出请求。
//! 桌面端权限属于主应用；授权请求通过临时后台角色执行。

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

/// Whether this process may use Accessibility.
pub fn accessibility_granted() -> bool {
    // SAFETY: no arguments; a pure query.
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Whether this process may record the screen. Looked up at run time because
/// `CGPreflightScreenCaptureAccess` only exists from macOS 10.15, and a
/// direct reference would stop iyw-claw from launching on anything older; there
/// the system has no Screen Recording permission to hold, so `false` is the
/// honest answer.
pub fn screen_recording_granted() -> bool {
    type Preflight = unsafe extern "C" fn() -> u8;
    static NAME: &[u8] = b"CGPreflightScreenCaptureAccess\0";
    // SAFETY: RTLD_DEFAULT lookup of a NUL-terminated name. CoreGraphics is
    // always loaded in a process that links AppKit, which iyw-claw and the
    // helper both do.
    let sym = unsafe { libc::dlsym(libc::RTLD_DEFAULT, NAME.as_ptr().cast()) };
    if sym.is_null() {
        return false;
    }
    // SAFETY: the symbol has exactly this signature.
    let preflight: Preflight = unsafe { std::mem::transmute::<*mut libc::c_void, Preflight>(sym) };
    // SAFETY: a pure query.
    unsafe { preflight() != 0 }
}

/// The process TCC charges this process's requests to, when the private
/// `responsibility_get_pid_responsible_for_pid` is available.
///
/// Used to tell a iyw-claw that holds a permission *itself* (launched from
/// Finder: it is its own responsible process) from a development build run
/// under a terminal, which reports the terminal's permissions — ones that
/// every process in that terminal, iyw-claw's agents included, already has.
pub fn responsible_pid(pid: u32) -> Option<u32> {
    type Responsible = unsafe extern "C" fn(libc::pid_t) -> libc::pid_t;
    static NAME: &[u8] = b"responsibility_get_pid_responsible_for_pid\0";
    // SAFETY: as above.
    let sym = unsafe { libc::dlsym(libc::RTLD_DEFAULT, NAME.as_ptr().cast()) };
    if sym.is_null() {
        return None;
    }
    // SAFETY: the symbol has exactly this signature.
    let f: Responsible = unsafe { std::mem::transmute::<*mut libc::c_void, Responsible>(sym) };
    let pid = libc::pid_t::try_from(pid).ok()?;
    // SAFETY: a pure query about a pid.
    let responsible = unsafe { f(pid) };
    u32::try_from(responsible).ok().filter(|p| *p > 0)
}
