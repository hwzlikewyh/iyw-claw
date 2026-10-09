// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Desktop {
    pub fn open() -> Self {
        let com = Com::enter();
        let mut object = ptr::null_mut();
        // SAFETY: constant class and interface ids, no outer object, and an
        // out-pointer that holds our one reference on success.
        let status = unsafe {
            CoCreateInstance(
                &CLSID_VIRTUAL_DESKTOP_MANAGER,
                ptr::null_mut(),
                CLSCTX_ALL,
                &IID_VIRTUAL_DESKTOP_MANAGER,
                &mut object,
            )
        };
        let desktops = if status >= 0 && !object.is_null() {
            Some(Object(object))
        } else {
            None
        };
        Self {
            desktops,
            _com: com,
        }
    }

    /// The process owning `window`; `None` when it names no window.
    pub fn owner(&self, window: u64) -> Option<u32> {
        owner(handle(window)?)
    }

    /// Whether the compositor hides `window` (see the module note).
    pub fn cloaked(&self, window: u64) -> bool {
        handle(window).is_some_and(cloaked)
    }

    /// Whether `window` is on the virtual desktop on the screen; `None` when
    /// the system will not say.
    pub fn on_current_desktop(&self, window: u64) -> Option<bool> {
        let desktops = self.desktops.as_ref()?;
        let window = handle(window)?;
        let mut on: BOOL = 0;
        // SAFETY: a live virtual desktop manager, a window handle (one that
        // names no window is answered with an error) and an out-value.
        let status = unsafe {
            (desktops
                .methods::<VirtualDesktopManagerMethods>()
                .is_window_on_current_virtual_desktop)(desktops.0, window, &mut on)
        };
        (status >= 0).then_some(on != 0)
    }

    /// The run of the process drawing inside `frame`, a window of the frame
    /// host `host` (see the module note); `None` when no process is, or when
    /// which one cannot be told. The run is read off the process it was found
    /// by — through the handle the core window is found still inside the
    /// frame and still that process's with, or the one its application was
    /// read through — so a pid that passed to another process in between
    /// cannot lend the frame that process's identity.
    pub fn frame_content(&self, frame: u64, host: u32) -> Option<ProcessRun> {
        let frame = handle(frame)?;
        let inside: Vec<(HWND, u32)> = core_windows(frame)
            .filter_map(|window| Some((window, owner(window)?)))
            .filter(|(_, pid)| *pid != host)
            .collect();
        match found(inside.iter().map(|(_, pid)| *pid)) {
            Found::One(pid) => {
                let window = inside.first()?.0;
                let started_at = process_start_while(pid, || {
                    owner(window) == Some(pid) && core_windows(frame).any(|w| w == window)
                })?;
                Some(ProcessRun { pid, started_at })
            }
            Found::Several => None,
            // Minimized: the core window stands on its own.
            Found::Nothing => {
                let shown = app_user_model_id(frame)?;
                let runs = core_windows(ptr::null_mut())
                    .filter_map(owner)
                    .filter_map(|pid| {
                        let image = process_image(pid)?;
                        (image.app_user_model_id.as_deref() == Some(shown.as_str())).then_some(
                            ProcessRun {
                                pid,
                                started_at: image.started,
                            },
                        )
                    });
                match found(runs) {
                    Found::One(run) => Some(run),
                    Found::Nothing | Found::Several => None,
                }
            }
        }
    }
}
