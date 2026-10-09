// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The kernel-held form of the pins: trycua's Developer ID, the driver's
/// identifier, one of the pinned builds.
#[cfg(target_os = "macos")]
pub(super) fn driver_launch_requirement() -> Result<Vec<u8>, HelperError> {
    crate::computer::launch_req::pinned_build(
        driver::DRIVER_TEAM_ID,
        driver::DRIVER_SIGNING_ID,
        driver::DRIVER_CDHASHES,
    )
    .ok_or_else(|| rejected("the driver's pinned cdhashes are malformed"))
}

/// Everything the running image must be. See the module note.
#[cfg(target_os = "macos")]
pub(super) fn verify_running_driver(pid: u32) -> Result<(), String> {
    use crate::computer::codesign::{
        check_guest, running_cdhash, running_status, Guest, CS_DEBUGGED, CS_GET_TASK_ALLOW,
        CS_RUNTIME, CS_VALID,
    };
    let info = check_guest(Guest::ChildPid(pid), &driver_requirement())
        .map_err(|e| format!("the driver's signature is not the pinned build's: {e}"))?;
    let cdhash =
        running_cdhash(pid).map_err(|e| format!("could not read the driver's cdhash: {e}"))?;
    if !driver::DRIVER_CDHASHES.contains(&cdhash.as_str()) {
        return Err(format!(
            "the running driver's cdhash {cdhash} is not a pinned build"
        ));
    }
    let status =
        running_status(pid).map_err(|e| format!("could not read the driver's status: {e}"))?;
    if status & CS_VALID == 0 {
        return Err("the running driver is not validly signed".into());
    }
    if status & CS_RUNTIME == 0 || info.flags.unwrap_or(0) & CS_RUNTIME == 0 {
        return Err("the driver does not run with the hardened runtime".into());
    }
    if status & (CS_GET_TASK_ALLOW | CS_DEBUGGED) != 0 {
        return Err("the driver can be attached to by a debugger".into());
    }
    driver::driver_entitlements_ok(&info.entitlements)
}
