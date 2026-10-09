// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The windows in a `list_windows` answer that could be someone's window at
/// all: the normal layer, with an area. Visible or not — whether a window is
/// worth *showing* is iyw-claw's call, made after it has matched the listing
/// against the windows it has already named. A shared window whose
/// application is hidden (⌘H) is off screen and still the same window, and a
/// listing that dropped it would read as the window closing and end the
/// grant.
pub(in crate::computer::helper) fn parse_windows(
    value: &Value,
) -> Result<Vec<RawWindow>, HelperError> {
    let flag = |w: &Value, key: &str| w.get(key).and_then(Value::as_bool);
    required_array("list_windows", value, "windows").map(|windows| {
        windows
            .iter()
            .filter(|w| w.get("layer").and_then(Value::as_i64).unwrap_or(0) == 0)
            .filter_map(|w| {
                let bounds = rect(w.get("bounds")?)?;
                if bounds.is_empty() {
                    return None;
                }
                let pid = u32::try_from(w.get("pid")?.as_u64()?).ok()?;
                Some(RawWindow {
                    window_id: w.get("window_id")?.as_u64()?,
                    pid,
                    title: string(w, "title").unwrap_or_default(),
                    bounds,
                    on_screen: flag(w, "is_on_screen").unwrap_or(false),
                    minimized: flag(w, "minimized"),
                    hidden: None,
                    on_current_space: flag(w, "on_current_space"),
                    z_index: w.get("z_index").and_then(Value::as_i64),
                    content: None,
                    app: RawApp {
                        pid,
                        name: string(w, "app_name").unwrap_or_default(),
                        bundle_id: None,
                        path: None,
                        active: false,
                        started_at: None,
                    },
                })
            })
            .collect()
    })
}
