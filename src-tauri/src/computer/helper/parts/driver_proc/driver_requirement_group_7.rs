// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The designated requirement the running driver must satisfy: trycua's
/// signing identity AND one of the pinned builds.
pub fn driver_requirement() -> String {
    let builds = driver::DRIVER_CDHASHES
        .iter()
        .map(|h| format!("cdhash H\"{h}\""))
        .collect::<Vec<_>>()
        .join(" or ");
    format!("({}) and ({builds})", driver::DRIVER_DESIGNATED_REQUIREMENT)
}
