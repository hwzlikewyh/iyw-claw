use std::fs::{File, Metadata};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileStamp {
    length: u64,
    modified: SystemTime,
    identity: [u64; 4],
}

impl FileStamp {
    pub(super) fn read(file: &File) -> Option<Self> {
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() {
            return None;
        }
        Some(Self {
            length: metadata.len(),
            modified: metadata.modified().ok()?,
            identity: identity(file, &metadata)?,
        })
    }
}

#[cfg(windows)]
fn identity(file: &File, _metadata: &Metadata) -> Option<[u64; 4]> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileBasicInfo, GetFileInformationByHandle, GetFileInformationByHandleEx,
        BY_HANDLE_FILE_INFORMATION, FILE_BASIC_INFO,
    };

    // 文件ID识别原子替换，ChangeTime识别保留mtime的原地修改。
    let (information, basic) = unsafe {
        let mut information: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        let mut basic: FILE_BASIC_INFO = std::mem::zeroed();
        let handle = file.as_raw_handle();
        if GetFileInformationByHandle(handle, &mut information) == 0
            || GetFileInformationByHandleEx(
                handle,
                FileBasicInfo,
                std::ptr::addr_of_mut!(basic).cast(),
                std::mem::size_of::<FILE_BASIC_INFO>() as u32,
            ) == 0
        {
            return None;
        }
        (information, basic)
    };
    Some([
        u64::from(information.dwVolumeSerialNumber),
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
        basic.CreationTime as u64,
        basic.ChangeTime as u64,
    ])
}

#[cfg(unix)]
fn identity(_file: &File, metadata: &Metadata) -> Option<[u64; 4]> {
    use std::os::unix::fs::MetadataExt;

    Some([
        metadata.dev(),
        metadata.ino(),
        metadata.ctime() as u64,
        metadata.ctime_nsec() as u64,
    ])
}

#[cfg(not(any(windows, unix)))]
fn identity(_file: &File, _metadata: &Metadata) -> Option<[u64; 4]> {
    None
}
