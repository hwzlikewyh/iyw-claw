use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{copy_nonoverlapping, null_mut};
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{GlobalFree, HGLOBAL, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};

const FILE_DROP_CLIPBOARD_FORMAT: u32 = 15;
const CLIPBOARD_OPEN_ATTEMPTS: usize = 4;
const CLIPBOARD_RETRY_DELAY: Duration = Duration::from_millis(20);

#[repr(C)]
struct DropFilesHeader {
    files_offset: u32,
    point_x: i32,
    point_y: i32,
    non_client: i32,
    wide: i32,
}

pub fn copy_file(path: &Path, owner: isize) -> io::Result<()> {
    let memory = build_drop_list(path)?;
    let _clipboard = open_clipboard(owner as HWND)?;
    if unsafe { EmptyClipboard() } == 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { SetClipboardData(FILE_DROP_CLIPBOARD_FORMAT, memory.handle()) }.is_null() {
        return Err(io::Error::last_os_error());
    }
    memory.release();
    Ok(())
}

fn build_drop_list(path: &Path) -> io::Result<OwnedGlobalMemory> {
    let mut wide_path = path.as_os_str().encode_wide().collect::<Vec<_>>();
    wide_path.extend([0, 0]);
    let header_size = size_of::<DropFilesHeader>();
    let path_size = wide_path
        .len()
        .checked_mul(size_of::<u16>())
        .ok_or_else(allocation_size_error)?;
    let total_size = header_size
        .checked_add(path_size)
        .ok_or_else(allocation_size_error)?;
    let memory = OwnedGlobalMemory::allocate(total_size)?;
    write_drop_list(&memory, &wide_path, header_size, path_size)?;
    Ok(memory)
}

fn write_drop_list(
    memory: &OwnedGlobalMemory,
    wide_path: &[u16],
    header_size: usize,
    path_size: usize,
) -> io::Result<()> {
    let files_offset = u32::try_from(header_size).map_err(|_| allocation_size_error())?;
    let header = DropFilesHeader {
        files_offset,
        point_x: 0,
        point_y: 0,
        non_client: 0,
        wide: 1,
    };
    let locked = unsafe { GlobalLock(memory.handle()) };
    if locked.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: GlobalLock 返回的可写内存已按头部和路径缓冲区总长度分配。
    unsafe {
        copy_nonoverlapping(
            (&header as *const DropFilesHeader).cast::<u8>(),
            locked.cast::<u8>(),
            header_size,
        );
        copy_nonoverlapping(
            wide_path.as_ptr().cast::<u8>(),
            locked.cast::<u8>().add(header_size),
            path_size,
        );
        let _ = GlobalUnlock(memory.handle());
    }
    Ok(())
}

fn open_clipboard(owner: HWND) -> io::Result<ClipboardGuard> {
    for attempt in 0..CLIPBOARD_OPEN_ATTEMPTS {
        if unsafe { OpenClipboard(owner) } != 0 {
            return Ok(ClipboardGuard);
        }
        if attempt + 1 < CLIPBOARD_OPEN_ATTEMPTS {
            thread::sleep(CLIPBOARD_RETRY_DELAY);
        }
    }
    Err(io::Error::last_os_error())
}

fn allocation_size_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "File path is too large")
}

struct ClipboardGuard;

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

struct OwnedGlobalMemory {
    handle: HGLOBAL,
}

impl OwnedGlobalMemory {
    fn allocate(size: usize) -> io::Result<Self> {
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { handle })
    }

    fn handle(&self) -> HGLOBAL {
        self.handle
    }

    fn release(mut self) {
        self.handle = null_mut();
    }
}

impl Drop for OwnedGlobalMemory {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                let _ = GlobalFree(self.handle);
            }
        }
    }
}
