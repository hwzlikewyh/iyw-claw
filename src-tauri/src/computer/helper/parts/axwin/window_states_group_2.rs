// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What each application says of its windows: for each of `listed`, whether
/// it is hidden and which of its windows are minimized; for each of `maybe`,
/// whether it is hidden — and its windows only if it is. An application that
/// would not answer (quit, hung, not an application) is left out.
pub async fn window_states(
    listed: BTreeSet<u32>,
    maybe: BTreeSet<u32>,
) -> HashMap<u32, AppWindows> {
    tokio::task::spawn_blocking(move || {
        let mut said = HashMap::new();
        for pid in listed.union(&maybe) {
            let Some(app) = application(*pid) else {
                continue;
            };
            let hidden = flag(&app, "AXHidden");
            if !listed.contains(pid) && hidden != Some(true) {
                continue;
            }
            let Ok(windows) = windows(&app) else {
                continue;
            };
            let minimized = windows
                .iter()
                .filter_map(|w| Some((window_number(w)?, flag(w, "AXMinimized"))))
                .collect();
            said.insert(*pid, AppWindows { hidden, minimized });
        }
        said
    })
    .await
    .unwrap_or_default()
}

/// Why a window is off the screen, as far as Accessibility tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutOfSight {
    Minimized,
    /// Its application is hidden (⌘H).
    AppHidden,
}

/// Whether `pid`'s window `window_id` is minimized or its application
/// hidden; `None` when it is neither, or that cannot be told.
pub async fn out_of_sight(pid: u32, window_id: u64) -> Option<OutOfSight> {
    tokio::task::spawn_blocking(move || {
        let app = application(pid)?;
        if flag(&app, "AXHidden") == Some(true) {
            return Some(OutOfSight::AppHidden);
        }
        let window = windows(&app)
            .ok()?
            .into_iter()
            .find(|w| window_number(w) == Some(window_id))?;
        (flag(&window, "AXMinimized") == Some(true)).then_some(OutOfSight::Minimized)
    })
    .await
    .ok()
    .flatten()
}

/// The windows `pid`'s application names as its focused one and as its
/// main one, by window id: what it brings forward when it is activated.
/// Each `None` where it names none, or would not say.
pub async fn focused_and_main(pid: u32) -> (Option<u64>, Option<u64>) {
    tokio::task::spawn_blocking(move || {
        let Some(app) = application(pid) else {
            return (None, None);
        };
        // SAFETY: a pure query.
        let element_type = unsafe { AXUIElementGetTypeID() };
        let named = |name: &'static str| {
            let window = attribute(&app, name).ok()?;
            // SAFETY: a live object, only asked its type.
            if unsafe { CFGetTypeID(window.as_CFTypeRef()) } != element_type {
                return None;
            }
            window_number(&window)
        };
        (named("AXFocusedWindow"), named("AXMainWindow"))
    })
    .await
    .unwrap_or((None, None))
}

/// What Accessibility says of a menu command before it is chosen: where its
/// first title sits on the menu bar — `0` is the Apple menu, `1` the
/// application menu — whether every title on the way was found, once each,
/// and the command's keyboard shortcut, as a key and the menu's modifier
/// mask, when it has one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuTarget {
    pub bar_index: Option<usize>,
    pub reached: bool,
    pub shortcut: Option<(String, i64)>,
}
