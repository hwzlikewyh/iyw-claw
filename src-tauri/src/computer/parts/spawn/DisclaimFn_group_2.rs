// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) type DisclaimFn = unsafe extern "C" fn(*mut libc::posix_spawnattr_t, c_int) -> c_int;

pub(super) fn disclaim_fn() -> Option<DisclaimFn> {
    static NAME: &[u8] = b"responsibility_spawnattrs_setdisclaim\0";
    // SAFETY: RTLD_DEFAULT with a NUL-terminated name; the symbol, when
    // present, has exactly this signature (libSystem, macOS 10.14+).
    let sym = unsafe { libc::dlsym(libc::RTLD_DEFAULT, NAME.as_ptr().cast()) };
    if sym.is_null() {
        None
    } else {
        // SAFETY: see above.
        Some(unsafe { std::mem::transmute::<*mut libc::c_void, DisclaimFn>(sym) })
    }
}
