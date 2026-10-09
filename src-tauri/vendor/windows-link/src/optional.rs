use core::ffi::{c_char, c_void};

#[cfg_attr(
    target_arch = "x86",
    link(
        name = "kernel32.dll",
        kind = "raw-dylib",
        modifiers = "+verbatim",
        import_name_type = "undecorated"
    )
)]
#[cfg_attr(
    not(target_arch = "x86"),
    link(name = "kernel32.dll", kind = "raw-dylib", modifiers = "+verbatim")
)]
unsafe extern "system" {
    fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *const c_void;
    fn SetLastError(error: u32);
}

/// 可选系统入口保持模块引用；缺失时返回失败，不模拟 OS 授权能力。
pub unsafe fn resolve_optional(library: &[u16], name: &[u8]) -> *const c_void {
    let mut module = unsafe { GetModuleHandleW(library.as_ptr()) };
    if module.is_null() {
        const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x800;
        module = unsafe {
            LoadLibraryExW(
                library.as_ptr(),
                core::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
    }
    let function = if module.is_null() {
        core::ptr::null()
    } else {
        unsafe { GetProcAddress(module, name.as_ptr().cast()) }
    };
    if function.is_null() {
        const ERROR_CALL_NOT_IMPLEMENTED: u32 = 120;
        unsafe { SetLastError(ERROR_CALL_NOT_IMPLEMENTED) };
    }
    function
}
