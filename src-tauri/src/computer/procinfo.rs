//! When a process started — the half of a process's identity a pid does not
//! carry.
//!
//! Pids are reused. A window grant bound to "pid 4312" would pass, after that
//! application quits, to whatever the system hands 4312 next; bound to "pid
//! 4312 that started at T" it cannot. The value is opaque and only ever
//! compared for equality, so each platform reports whatever it has most
//! cheaply: microseconds since the epoch on macOS, clock ticks since boot on
//! Linux, a FILETIME on Windows.
//!
//! None of this is TCC-governed on macOS: `proc_pidinfo` on another user
//! process of the same user is an ordinary BSD query.
//!
//! On Windows the same handle also says which executable a process runs, and
//! which packaged application it is, if any (`process_image`) — which is how
//! the helper tells applications apart there, and names them (see
//! `appident`).

/// `pid`'s start stamp, or `None` when it is not running (or the platform
/// will not say).
pub fn process_start(pid: u32) -> Option<u64> {
    if pid == 0 {
        return None;
    }
    imp::process_start(pid)
}

/// Windows: `pid`'s start stamp, read through a handle held open while
/// `still` is asked — so a `still` that finds `pid` where it was found before
/// (a window's owner, say) has found this very run of it: the system does not
/// hand a pid to another process while a handle to it is open. `None` when it
/// is not running, the system will not say, or `still` answers `false`.
#[cfg(windows)]
pub fn process_start_while(pid: u32, still: impl FnOnce() -> bool) -> Option<u64> {
    if pid == 0 {
        return None;
    }
    imp::process_start_while(pid, still)
}

/// Windows: what `pid` runs, read through one handle — so all of it is the
/// same process's, even should the pid pass to another process in between.
/// `None` when it is not running, or the system will not say.
#[cfg(windows)]
pub fn process_image(pid: u32) -> Option<ProcessImage> {
    if pid == 0 {
        return None;
    }
    imp::process_image(pid)
}

/// Windows: what a process runs.
#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessImage {
    /// Its start stamp, as [`process_start`] gives it.
    pub started: u64,
    /// The full path of its executable.
    pub path: String,
    /// The application user model id of the packaged application it runs
    /// (`Microsoft.WindowsTerminal_8wekyb3d8bbwe!App`); `None` for a process
    /// that is no package's application.
    pub app_user_model_id: Option<String>,
}

#[cfg(target_os = "macos")]
mod imp {
    pub fn process_start(pid: u32) -> Option<u64> {
        let pid = libc::c_int::try_from(pid).ok()?;
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        // SAFETY: `info` is a zeroed buffer of exactly `size` bytes, which is
        // what `proc_pidinfo` fills for PROC_PIDTBSDINFO; it returns the byte
        // count written, and anything short of the whole struct is treated as
        // failure before the buffer is read.
        let written = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        if written != size {
            return None;
        }
        // SAFETY: fully written, checked above.
        let info = unsafe { info.assume_init() };
        Some(
            info.pbi_start_tvsec
                .saturating_mul(1_000_000)
                .saturating_add(info.pbi_start_tvusec),
        )
    }
}

#[cfg(target_os = "linux")]
mod imp {
    pub fn process_start(pid: u32) -> Option<u64> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        parse_stat_start(&stat)
    }

    /// Field 22 of `/proc/<pid>/stat` (`starttime`). The second field is the
    /// command name in parentheses and may itself contain spaces and
    /// parentheses, so fields are counted from the LAST `)`.
    pub(super) fn parse_stat_start(stat: &str) -> Option<u64> {
        let after_comm = &stat[stat.rfind(')')? + 1..];
        // After the comm come field 3 (state) onward; starttime is field 22,
        // i.e. the 20th whitespace-separated token from here.
        after_comm.split_whitespace().nth(19)?.parse().ok()
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// The longest path the system runs a program from, in UTF-16 units.
    const MAX_IMAGE_PATH: usize = 32_768;

    /// The longest application user model id, in UTF-16 units with its
    /// terminating NUL (`APPLICATION_USER_MODEL_ID_MAX_LENGTH`).
    const MAX_APP_USER_MODEL_ID: usize = 130;

    // Declared here: windows-sys has it behind a feature this crate does not
    // turn on (`Win32_Storage_Packaging_Appx`), and turning one on rebuilds
    // every crate that shares windows-sys — Tauri among them.
    #[link(name = "kernel32")]
    extern "system" {}

    /// A process opened for the few questions any process of this user
    /// answers, closed when dropped. While it is open the system does not
    /// hand the pid to another process.
    struct Process(HANDLE);

    impl Process {
        fn open(pid: u32) -> Option<Self> {
            // SAFETY: OpenProcess returns a handle we own, or null.
            let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            // Only a handle that was opened is wrapped: the wrapper closes
            // whatever it holds.
            if handle.is_null() {
                None
            } else {
                Some(Self(handle))
            }
        }

        fn start(&self) -> Option<u64> {
            let zero = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
            // SAFETY: four valid out-pointers and a live handle.
            let ok = unsafe {
                GetProcessTimes(self.0, &mut created, &mut exited, &mut kernel, &mut user)
            };
            (ok != 0).then(|| {
                (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime)
            })
        }

        /// The executable's full path, in the form the rest of the system
        /// spells paths (`C:\…`). `None` for one that is not valid UTF-16,
        /// which could not be told apart from another once made readable.
        fn image(&self) -> Option<String> {
            let mut buf = vec![0u16; MAX_IMAGE_PATH];
            let mut len = MAX_IMAGE_PATH as u32;
            // SAFETY: `buf` holds `len` units; on success `len` is set to the
            // number written, not counting the terminating NUL.
            let ok = unsafe {
                QueryFullProcessImageNameW(self.0, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
            };
            if ok == 0 || len == 0 {
                return None;
            }
            buf.truncate(len as usize);
            String::from_utf16(&buf).ok()
        }

        /// The application user model id of the packaged application the
        /// process runs; `None` for one that is no package's application,
        /// which the call answers with an error.
        fn app_user_model_id(&self) -> Option<String> {
            let query = application_user_model_id_query()?;
            let mut buf = [0u16; MAX_APP_USER_MODEL_ID];
            let mut len = MAX_APP_USER_MODEL_ID as u32;
            // SAFETY: `buf` holds `len` units; on success `len` is set to the
            // number written, counting the terminating NUL.
            let status = unsafe { query(self.0, &mut len, buf.as_mut_ptr()) };
            if status != 0 {
                return None;
            }
            let written = buf.get(..len as usize)?;
            let id = written.split(|unit| *unit == 0).next()?;
            String::from_utf16(id).ok().filter(|id| !id.is_empty())
        }
    }

    type ApplicationIdQuery = unsafe extern "system" fn(HANDLE, *mut u32, *mut u16) -> u32;

    // Windows 7 没有此入口；动态解析避免整个应用在加载时失败。
    fn application_user_model_id_query() -> Option<ApplicationIdQuery> {
        use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
        static QUERY: std::sync::OnceLock<Option<ApplicationIdQuery>> = std::sync::OnceLock::new();
        *QUERY.get_or_init(|| {
            let module_name: Vec<u16> = "kernel32.dll".encode_utf16().chain(Some(0)).collect();
            // SAFETY: 系统已加载的模块；符号签名来自 Windows API。
            unsafe {
                let module = GetModuleHandleW(module_name.as_ptr());
                if module.is_null() {
                    return None;
                }
                GetProcAddress(module, c"GetApplicationUserModelId".as_ptr().cast()).map(
                    |address| {
                        std::mem::transmute::<
                            unsafe extern "system" fn() -> isize,
                            ApplicationIdQuery,
                        >(address)
                    },
                )
            }
        })
    }

    impl Drop for Process {
        fn drop(&mut self) {
            // SAFETY: the handle opened in `open`, closed exactly once.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn process_start(pid: u32) -> Option<u64> {
        Process::open(pid)?.start()
    }

    pub fn process_start_while(pid: u32, still: impl FnOnce() -> bool) -> Option<u64> {
        let process = Process::open(pid)?;
        let started = process.start()?;
        // Asked with the handle still open: see `process_start_while`.
        still().then_some(started)
    }

    pub fn process_image(pid: u32) -> Option<super::ProcessImage> {
        let process = Process::open(pid)?;
        Some(super::ProcessImage {
            started: process.start()?,
            path: process.image()?,
            app_user_model_id: process.app_user_model_id(),
        })
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod imp {
    pub fn process_start(_pid: u32) -> Option<u64> {
        None
    }
}
