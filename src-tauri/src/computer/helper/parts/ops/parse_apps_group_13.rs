// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(any(test, not(any(target_os = "macos", windows))))]
pub(super) fn parse_apps(value: &Value) -> Result<Vec<RawApp>, HelperError> {
    required_array("list_apps", value, "apps").map(|apps| {
        apps.iter()
            // The driver also lists installed applications that are not
            // running (pid 0); only running ones have windows.
            .filter(|a| a.get("running").and_then(Value::as_bool) == Some(true))
            .filter_map(|a| {
                let pid = u32::try_from(a.get("pid")?.as_u64()?)
                    .ok()
                    .filter(|p| *p > 0)?;
                Some(RawApp {
                    pid,
                    name: string(a, "name").unwrap_or_default(),
                    bundle_id: string(a, "bundle_id"),
                    path: string(a, "launch_path"),
                    active: a.get("active").and_then(Value::as_bool).unwrap_or(false),
                    started_at: process_start(pid),
                })
            })
            .collect()
    })
}
