// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Normal windows, each joined with its application.
pub async fn list_windows(
    driver: &DriverProc,
    cache: &tokio::sync::Mutex<AppCache>,
    pid: Option<u32>,
) -> Result<Vec<RawWindow>, HelperError> {
    let mut args = json!({ "on_screen_only": false });
    if let Some(pid) = pid {
        args["pid"] = json!(pid);
    }
    let result = call(driver, "list_windows", args, LIST_TIMEOUT).await?;
    let windows = parse_windows(structured("list_windows", &result)?)?;

    let stamps: Vec<Option<u64>> = windows.iter().map(|w| process_start(w.pid)).collect();
    #[cfg(target_os = "macos")]
    {
        let _ = cache;
        Ok(join_identified(windows, stamps))
    }
    // Naming a packaged application the first time can take the Start menu
    // a fifth of a second: off the runtime's two threads.
    #[cfg(windows)]
    {
        let _ = cache;
        tokio::task::spawn_blocking(move || join_identified(windows, stamps))
            .await
            .map_err(|e| HelperError::failed(format!("the windows' owners could not be read: {e}")))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let mut cache = cache.lock().await;
        let missing = windows
            .iter()
            .zip(&stamps)
            .any(|(w, started)| cache.lookup(w.pid, *started).is_none());
        if missing {
            cache.refill(driver_apps(driver).await?);
            // A process the application list does not know — a background
            // helper, an agent's own window — keeps the name the window list
            // gave it and has no key a blocklist could match. Remembered like
            // any other, so it does not send every later listing back to the
            // slow application list.
            for (window, started_at) in windows.iter().zip(&stamps) {
                cache
                    .apps
                    .entry((window.pid, *started_at))
                    .or_insert_with(|| RawApp {
                        started_at: *started_at,
                        ..window.app.clone()
                    });
            }
        }
        Ok(windows
            .into_iter()
            .zip(stamps)
            .map(|(mut window, started_at)| {
                if let Some(app) = cache.lookup(window.pid, started_at) {
                    window.app = app.clone();
                }
                window.app.started_at = started_at;
                window
            })
            .collect())
    }
}
