// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::ffi::CStr;
use windows::Win32::Foundation::FreeLibrary;
use windows::Win32::System::Diagnostics::Debug::{
    GetThreadErrorMode, SetThreadErrorMode, SEM_FAILCRITICALERRORS, THREAD_ERROR_MODE,
};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows_core::{s, w, BOOL, PCSTR};

type IsApiSetImplemented = unsafe extern "system" fn(PCSTR) -> BOOL;

pub(crate) fn is_api_set_implemented(contract: &CStr) -> bool {
    // 可选 API 缺失时不弹系统错误框；只修改并恢复当前线程的错误模式。
    unsafe {
        let previous = THREAD_ERROR_MODE(GetThreadErrorMode());
        if SetThreadErrorMode(previous | SEM_FAILCRITICALERRORS, None).is_err() {
            return false;
        }
        let implemented = query(contract);
        let _ = SetThreadErrorMode(previous, None);
        implemented
    }
}

fn query(contract: &CStr) -> bool {
    // 句柄保持到函数调用结束，仅从系统目录解析，避免把可选 API 变成启动依赖。
    unsafe {
        let Ok(module) = LoadLibraryExW(
            w!("api-ms-win-core-apiquery-l2-1-0.dll"),
            None,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        ) else {
            return false;
        };
        let implemented = GetProcAddress(module, s!("IsApiSetImplemented"))
            .map(|address| {
                let query = std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    IsApiSetImplemented,
                >(address);
                query(PCSTR(contract.as_ptr().cast())).as_bool()
            })
            .unwrap_or(false);
        let _ = FreeLibrary(module);
        implemented
    }
}
