// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether the listing leaves open if `window` is minimized when it could be:
/// off screen, yet on a Space. (One on screen is not; one on no Space at all
/// is the furniture every application keeps.)
#[cfg(any(test, target_os = "macos"))]
pub(super) fn unexplained(window: &RawWindow) -> bool {
    !window.on_screen && window.minimized.is_none() && window.on_current_space.is_some()
}

/// Which applications Accessibility is asked about, and how much: those
/// with a window [`unexplained`], about all their windows (`listed`); those
/// with nothing on the screen and a window off it, only whether they are
/// hidden — and their windows only if they are (`maybe`): a hidden
/// application's windows may be on no Space, like the furniture every
/// application keeps.
#[cfg(any(test, target_os = "macos"))]
pub(super) fn owners_to_ask(
    windows: &[RawWindow],
) -> (
    std::collections::BTreeSet<u32>,
    std::collections::BTreeSet<u32>,
) {
    let listed: std::collections::BTreeSet<u32> = windows
        .iter()
        .filter(|w| unexplained(w))
        .map(|w| w.pid)
        .collect();
    let showing: std::collections::BTreeSet<u32> = windows
        .iter()
        .filter(|w| w.on_screen)
        .map(|w| w.pid)
        .collect();
    let maybe = windows
        .iter()
        .filter(|w| !w.on_screen && w.minimized.is_none())
        .map(|w| w.pid)
        .filter(|pid| !showing.contains(pid) && !listed.contains(pid))
        .collect();
    (listed, maybe)
}

/// Write down what each application said of its windows (`said`, by pid) for
/// the windows the listing left open — off screen, and not said to be
/// minimized or not. A window its application did not list stays unsaid: not
/// one a person can bring up. One on no Space is written down only when its
/// application is hidden — otherwise it is furniture, whatever is said of it.
#[cfg(any(test, target_os = "macos"))]
pub(super) fn settle(windows: &mut [RawWindow], said: &HashMap<u32, AppWindows>) {
    for window in windows
        .iter_mut()
        .filter(|w| !w.on_screen && w.minimized.is_none())
    {
        let Some(app) = said.get(&window.pid) else {
            continue;
        };
        let Some(minimized) = app.minimized.get(&window.window_id) else {
            continue;
        };
        let hidden = app.hidden == Some(true);
        if window.on_current_space.is_none() && !hidden {
            continue;
        }
        window.minimized = *minimized;
        if hidden {
            window.hidden = Some(true);
        }
    }
}
