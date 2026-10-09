// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Windows: join each window with its application, and say of it what the
/// listing leaves unsaid (see `crate::computer::helper::hwnd`): a window the compositor hides
/// is off the screen — on another virtual desktop, or out of sight on this
/// one; one the system will not place stays as the listing had it. What the
/// system says of a window counts only while its handle is still the listed
/// process's: a window closed since, its handle handed on, says nothing of
/// the one listed.
///
/// The application is the owning process's executable, read off the process
/// with its start time through one handle, which must still be the start time
/// the window list's owner had (a pid reused in between would lend the window
/// another application's identity) — or, for a frame, the executable of the
/// process drawing inside it, read the same way. Each is named as Windows
/// names it to the person (see `appident`), and read once per listing.
/// Unidentified (no path, the window list's name) when that cannot be read,
/// or when the process runs no application: a host other than the frame
/// host, or one of the system's own agents.
#[cfg(windows)]
pub(in crate::computer::helper) fn join_identified(
    windows: Vec<RawWindow>,
    stamps: Vec<Option<u64>>,
) -> Vec<RawWindow> {
    use crate::computer::appident::{windows_application, windows_owner, WindowsApp, WindowsOwner};
    use crate::computer::protocol::ProcessRun;

    let desktop = crate::computer::helper::hwnd::Desktop::open();
    let mut owners: HashMap<(u32, u64), WindowsOwner> = HashMap::new();
    let mut contents: HashMap<ProcessRun, Option<WindowsApp>> = HashMap::new();
    windows
        .into_iter()
        .zip(stamps)
        .map(|(mut window, started_at)| {
            let held = desktop.owner(window.window_id) == Some(window.pid);
            if held && desktop.cloaked(window.window_id) {
                if let Some(here) = desktop.on_current_desktop(window.window_id) {
                    window.on_screen = false;
                    window.on_current_space = Some(here);
                }
            }
            let owner = started_at.map(|started_at| {
                owners
                    .entry((window.pid, started_at))
                    .or_insert_with(|| windows_owner(window.pid, started_at))
                    .clone()
            });
            let app = match owner {
                Some(WindowsOwner::Application(app)) => Some(app),
                Some(WindowsOwner::FrameHost) if held => {
                    window.content = desktop.frame_content(window.window_id, window.pid);
                    window.content.and_then(|run| {
                        contents
                            .entry(run)
                            .or_insert_with(|| windows_application(run.pid, run.started_at))
                            .clone()
                    })
                }
                _ => None,
            };
            let (name, path) = match app {
                Some(app) => (app.name, Some(app.path)),
                None => (std::mem::take(&mut window.app.name), None),
            };
            window.app = RawApp {
                pid: window.pid,
                name,
                bundle_id: None,
                path,
                active: false,
                started_at,
            };
            window
        })
        .collect()
}
