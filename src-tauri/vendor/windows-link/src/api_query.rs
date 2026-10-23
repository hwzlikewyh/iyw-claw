use core::ffi::c_void;

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
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *const c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
    fn GetThreadErrorMode() -> u32;
    fn SetThreadErrorMode(mode: u32, previous: *mut u32) -> i32;
}

const API_QUERY_LIBRARY: &[u16] = &{
    const NAME: &str = "api-ms-win-core-apiquery-l2-1-0.dll";
    let mut wide = [0u16; NAME.len() + 1];
    let mut index = 0;
    while index < NAME.len() {
        wide[index] = NAME.as_bytes()[index] as u16;
        index += 1;
    }
    wide
};
const SEM_FAILCRITICALERRORS: u32 = 0x0001;
const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0800;

/// # Safety
/// `contract` must point to a valid null-terminated API-set name.
#[doc(hidden)]
pub unsafe fn query_api_set(contract: *const u8) -> i32 {
    // 可选 DLL 缺失时不弹系统错误框，并恢复调用线程的原错误模式。
    let previous = unsafe { GetThreadErrorMode() };
    if unsafe { SetThreadErrorMode(previous | SEM_FAILCRITICALERRORS, core::ptr::null_mut()) } == 0
    {
        return 0;
    }
    let supported = unsafe { query(contract) };
    let _ = unsafe { SetThreadErrorMode(previous, core::ptr::null_mut()) };
    supported
}

unsafe fn query(contract: *const u8) -> i32 {
    // 仅从系统目录加载，句柄保留到 API 调用完成，缺失时如实返回不支持。
    let module = unsafe {
        LoadLibraryExW(
            API_QUERY_LIBRARY.as_ptr(),
            core::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    };
    if module.is_null() {
        return 0;
    }
    let address = unsafe { GetProcAddress(module, b"IsApiSetImplemented\0".as_ptr()) };
    let supported = if address.is_null() {
        0
    } else {
        let query = unsafe {
            core::mem::transmute::<*const c_void, unsafe extern "system" fn(*const u8) -> i32>(
                address,
            )
        };
        unsafe { query(contract) }
    };
    let _ = unsafe { FreeLibrary(module) };
    supported
}
