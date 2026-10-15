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
            GetClientRect, SIZE_MINIMIZED, WM_DESTROY, WM_ENTERSIZEMOVE, WM_MOVE, WM_MOVING,
            WM_NCDESTROY, WM_SETFOCUS, WM_SIZE,
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
    if matches!(message, WM_DESTROY | WM_NCDESTROY) {
        RemoveWindowSubclass(hwnd, Some(window_proc), HOOK_ID);
        drop(Box::from_raw(held as *mut ICoreWebView2Controller));
        return DefSubclassProc(hwnd, message, wparam, lparam);
    }
    if !matches!(
        message,
        WM_SIZE | WM_SETFOCUS | WM_ENTERSIZEMOVE | WM_MOVE | WM_MOVING
    ) {
        return DefSubclassProc(hwnd, message, wparam, lparam);
    }
    // COM 调用可能重入窗口销毁；局部引用不能依赖钩子 userdata 继续存活。
    let controller = (&*(held as *const ICoreWebView2Controller)).clone();
    match message {
        WM_SIZE if wparam != SIZE_MINIMIZED as usize => resize(hwnd, &controller),
        WM_SETFOCUS | WM_ENTERSIZEMOVE => {
            let _ = controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
        }
        WM_MOVE | WM_MOVING => {
            let _ = controller.NotifyParentWindowPositionChanged();
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
    // 只同步主视图，不能用客户区尺寸再次缩放主窗口并触发新的 WM_SIZE。
    let _ = controller.SetBounds(windows::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    });
}
