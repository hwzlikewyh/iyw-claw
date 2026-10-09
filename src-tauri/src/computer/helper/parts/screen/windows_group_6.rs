// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The windows on the screen, judged by `rules`.
pub async fn windows(rules: &ScreenRules) -> Result<Vec<ScreenWindow>, HelperError> {
    let blocklist = Blocklist::from_entries(&rules.blocklist);
    let me = rules.me.clone();
    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(move || {
            let mut apps: std::collections::HashMap<u32, crate::computer::protocol::RawApp> =
                Default::default();
            window_server_windows()
                .into_iter()
                .map(|(id, pid, layer, bounds)| {
                    let app = apps.entry(pid).or_insert_with(|| {
                        let started_at = crate::computer::procinfo::process_start(pid);
                        crate::computer::helper::ops::identified(pid, started_at, "")
                    });
                    let allowed = !shows_others(app.bundle_id.as_deref(), layer)
                        && grantable(app, &me, &blocklist).is_ok();
                    // Whether clicks pass through cannot be told here: every
                    // window is taken to take them.
                    ScreenWindow {
                        id,
                        bounds,
                        allowed,
                        takes_clicks: true,
                    }
                })
                .collect()
        })
        .await
        .map_err(|e| HelperError::failed(format!("the screen's windows could not be read: {e}")))
    }
    #[cfg(windows)]
    {
        use crate::computer::protocol::{RawApp, RawWindow};
        tokio::task::spawn_blocking(move || {
            let listed = crate::computer::helper::hwnd::screen_windows();
            let seen: Vec<(bool, bool)> = listed
                .iter()
                .map(|w| (shows_others(&w.class), !w.passes_clicks))
                .collect();
            let shown: Vec<RawWindow> = listed
                .into_iter()
                .map(|w| RawWindow {
                    window_id: w.id,
                    pid: w.pid,
                    title: String::new(),
                    bounds: w.frame,
                    on_screen: true,
                    minimized: Some(false),
                    hidden: None,
                    on_current_space: None,
                    z_index: None,
                    content: None,
                    app: RawApp {
                        pid: w.pid,
                        name: String::new(),
                        bundle_id: None,
                        path: None,
                        active: false,
                        started_at: None,
                    },
                })
                .collect();
            let stamps = shown
                .iter()
                .map(|w| crate::computer::procinfo::process_start(w.pid))
                .collect();
            crate::computer::helper::ops::join_identified(shown, stamps)
                .into_iter()
                .zip(seen)
                .filter(|(w, _)| w.on_screen)
                .map(|(w, (overview, takes_clicks))| ScreenWindow {
                    id: w.window_id,
                    bounds: w.bounds,
                    allowed: !overview && grantable(&w.app, &me, &blocklist).is_ok(),
                    takes_clicks,
                })
                .collect()
        })
        .await
        .map_err(|e| HelperError::failed(format!("the screen's windows could not be read: {e}")))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (me, blocklist);
        Err(HelperError::failed(
            "The entire screen is not offered on this system: share windows or applications \
             instead.",
        ))
    }
}
