//! Wry 销毁子视图时会移除父窗口钩子，单独保留主视图的尺寸和焦点同步。
use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2Controller, COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GetClientRect, SetWindowPos, SIZE_MINIMIZED, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
            SWP_NOZORDER, WM_ENTERSIZEMOVE, WM_MOVE, WM_MOVING, WM_NCDESTROY, WM_SETFOCUS, WM_SIZE,
        },
    },
};

// 独立于 Wry 的子类 ID；钩子的过程地址也参与标识。
const HOOK_ID: usize = 0x495957;

pub(crate) fn retain(owner: &WebviewWindow) -> Result<(), String> {
    let hwnd = owner.hwnd().map_err(|error| error.to_string())?.0 as usize;
    let label = owner.label().to_string();
    owner
        .with_webview(move |platform| unsafe {
            let hwnd = hwnd as HWND;
            if GetWindowSubclass(hwnd, Some(window_proc), HOOK_ID, std::ptr::null_mut()) != 0 {
                return;
            }
            let controller = Box::into_raw(Box::new(platform.controller()));
            if SetWindowSubclass(hwnd, Some(window_proc), HOOK_ID, controller as usize) == 0 {
                drop(Box::from_raw(controller));
                tracing::warn!(owner = label, error = %std::io::Error::last_os_error(),
                    "native page parent window hook failed");
            }
        })
        .map_err(|error| error.to_string())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    held: usize,
) -> LRESULT {
    if held == 0 {
        return DefSubclassProc(hwnd, message, wparam, lparam);
    }
    let controller = &*(held as *const ICoreWebView2Controller);
    match message {
        WM_SIZE if wparam != SIZE_MINIMIZED as usize => resize(hwnd, controller),
        WM_SETFOCUS | WM_ENTERSIZEMOVE => {
            let _ = controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
        }
        WM_MOVE | WM_MOVING => {
            let _ = controller.NotifyParentWindowPositionChanged();
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(window_proc), HOOK_ID);
            drop(Box::from_raw(held as *mut ICoreWebView2Controller));
        }
        _ => {}
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}

unsafe fn resize(hwnd: HWND, controller: &ICoreWebView2Controller) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if GetClientRect(hwnd, &mut rect) == 0 {
        return;
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let _ = controller.SetBounds(windows::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    });
    let mut host = windows::Win32::Foundation::HWND::default();
    if controller.ParentWindow(&mut host).is_ok() {
        SetWindowPos(
            host.0,
            std::ptr::null_mut(),
            0,
            0,
            width,
            height,
            SWP_ASYNCWINDOWPOS | SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
}
