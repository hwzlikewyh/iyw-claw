use std::ffi::c_void;
use std::ptr;

use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoExW, GetFileVersionInfoSizeExW, VerQueryValueW, FILE_VER_GET_LOCALISED,
};
use windows_sys::Win32::UI::Shell::SIGDN_NORMALDISPLAY;

/// The largest version resource read: real ones are a few kilobytes, and
/// the helper does not read an unbounded one because an executable says
/// so.
const MAX_VERSION_INFO: u32 = 1 << 20;

/// Where a version resource's strings are looked for when it does not
/// say which languages it has them in: US English, then no language,
/// each as Unicode and as Windows' Western code page.
const FALLBACK_TRANSLATIONS: [(u16, u16); 4] = [
    (0x0409, 0x04b0),
    (0x0409, 0x04e4),
    (0x0000, 0x04b0),
    (0x0000, 0x04e4),
];

/// `COINIT_MULTITHREADED`.
const COINIT_MULTITHREADED: u32 = 0;

// Declared here: windows-sys has these behind features this crate does
// not turn on (`Win32_System_Com`, `Win32_UI_Shell_Common`), and turning
// one on rebuilds every crate that shares windows-sys — Tauri among them.
#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *const c_void, co_init: u32) -> i32;
    fn CoUninitialize();
    fn CoTaskMemFree(block: *const c_void);
}
#[link(name = "shell32")]
extern "system" {
    fn SHParseDisplayName(
        name: *const u16,
        bind_context: *mut c_void,
        id_list: *mut *mut c_void,
        attributes_asked: u32,
        attributes: *mut u32,
    ) -> i32;
    fn SHGetNameFromIDList(id_list: *const c_void, kind: i32, name: *mut *mut u16) -> i32;
    fn ILFree(id_list: *const c_void);
}

/// What the Start menu calls the packaged application
/// `app_user_model_id`: the display name of its entry in the shell's
/// Apps folder, which is in the person's language.
pub fn start_menu_name(app_user_model_id: &str) -> Option<String> {
    // Declared first, so the list and the string below are let go of
    // while COM is still entered.
    let _com = Com::enter();
    let item = wide(&format!(r"shell:AppsFolder\{app_user_model_id}"));
    let mut id_list = ptr::null_mut();
    // SAFETY: a NUL-terminated name, no bind context, and an out-pointer
    // for the item's id list, which is ours to free once it is set.
    let status = unsafe {
        SHParseDisplayName(
            item.as_ptr(),
            ptr::null_mut(),
            &mut id_list,
            0,
            ptr::null_mut(),
        )
    };
    if status < 0 || id_list.is_null() {
        return None;
    }
    let id_list = IdList(id_list);
    let mut name = ptr::null_mut();
    // SAFETY: a live id list; on success `name` is a NUL-terminated
    // string of the caller's to free.
    let status = unsafe { SHGetNameFromIDList(id_list.0, SIGDN_NORMALDISPLAY, &mut name) };
    if status < 0 || name.is_null() {
        return None;
    }
    let name = TaskString(name);
    trimmed(&name.units())
}

/// The description in the version resource of the executable at `path`,
/// in the person's language where Windows carries a translation of it.
pub fn file_description(path: &str) -> Option<String> {
    let path = wide(path);
    let mut unused = 0;
    // SAFETY: a NUL-terminated path and a valid out-pointer.
    let size =
        unsafe { GetFileVersionInfoSizeExW(FILE_VER_GET_LOCALISED, path.as_ptr(), &mut unused) };
    if size == 0 || size > MAX_VERSION_INFO {
        return None;
    }
    let mut block = vec![0u8; size as usize];
    // SAFETY: `block` holds `size` bytes, the size just asked for.
    let read = unsafe {
        GetFileVersionInfoExW(
            FILE_VER_GET_LOCALISED,
            path.as_ptr(),
            0,
            size,
            block.as_mut_ptr().cast(),
        )
    };
    if read == 0 {
        return None;
    }
    // The languages the strings are in, as (language, code page) pairs.
    let listed: Vec<(u16, u16)> = value(&block, r"\VarFileInfo\Translation", |bytes| bytes)
        .map(|bytes| {
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&[l0, l1, c0, c1]| {
                    (u16::from_le_bytes([l0, l1]), u16::from_le_bytes([c0, c1]))
                })
                .collect()
        })
        .unwrap_or_default();
    listed
        .into_iter()
        .chain(FALLBACK_TRANSLATIONS)
        .find_map(|(language, code_page)| {
            let key = format!(r"\StringFileInfo\{language:04X}{code_page:04X}\FileDescription");
            // A string's length is given in UTF-16 units.
            let bytes = value(&block, &key, |units| units.saturating_mul(2))?;
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&unit| u16::from_le_bytes(unit))
                .collect();
            trimmed(&units)
        })
}

/// The value at `sub_block` of the version resource read into `block`,
/// as the bytes it spans there — `length` turns the length the call gives
/// into bytes. `None` when there is no such value, or it would reach past
/// the resource.
fn value<'a>(
    block: &'a [u8],
    sub_block: &str,
    length: impl Fn(usize) -> usize,
) -> Option<&'a [u8]> {
    let sub_block = wide(sub_block);
    let mut found: *mut c_void = ptr::null_mut();
    let mut len = 0u32;
    // SAFETY: `block` is the resource GetFileVersionInfoExW filled, and
    // both out-pointers are valid; what `found` points at is read only
    // below, once it is known to lie within `block`.
    let ok = unsafe {
        VerQueryValueW(
            block.as_ptr().cast(),
            sub_block.as_ptr(),
            &mut found,
            &mut len,
        )
    };
    if ok == 0 || found.is_null() {
        return None;
    }
    let start = (found as usize).checked_sub(block.as_ptr() as usize)?;
    block.get(start..start.checked_add(length(len as usize))?)
}

/// `units` up to the first NUL, trimmed; `None` when nothing is left.
fn trimmed(units: &[u16]) -> Option<String> {
    let units = units.split(|unit| *unit == 0).next().unwrap_or(units);
    let text = String::from_utf16_lossy(units);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// COM, entered on this thread while the guard lives: the shell's names
/// are read through it, and the virtual desktops asked (see
/// `helper::hwnd`). A thread already in COM in the other mode stays so,
/// which serves as well.
pub(crate) struct Com(bool);

impl Com {
    pub(crate) fn enter() -> Self {
        // SAFETY: no reserved pointer; a success (S_OK, or S_FALSE when
        // already entered) is balanced in `drop`.
        let status = unsafe { CoInitializeEx(ptr::null(), COINIT_MULTITHREADED) };
        Self(status >= 0)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the CoInitializeEx that succeeded.
            unsafe { CoUninitialize() };
        }
    }
}

/// An item id list the shell allocated, freed when dropped.
struct IdList(*mut c_void);

impl Drop for IdList {
    fn drop(&mut self) {
        // SAFETY: the list SHParseDisplayName returned, freed once.
        unsafe { ILFree(self.0) };
    }
}

/// A string the shell allocated with COM's allocator, freed when dropped.
struct TaskString(*mut u16);

impl TaskString {
    fn units(&self) -> Vec<u16> {
        let mut len = 0;
        // SAFETY: a NUL-terminated string, read up to its NUL.
        while unsafe { *self.0.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: the `len` units just read before the NUL.
        unsafe { std::slice::from_raw_parts(self.0, len) }.to_vec()
    }
}

impl Drop for TaskString {
    fn drop(&mut self) {
        // SAFETY: the string SHGetNameFromIDList returned, freed once.
        unsafe { CoTaskMemFree(self.0.cast()) };
    }
}
