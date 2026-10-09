// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Put the window back on the screen, then watch for it there: confirmed
/// once the driver lists it on screen, unverifiable if it has not by the time
/// [`RESTORE_WAIT`] has passed (a look already asked is answered first,
/// however long the driver takes). A window already on the screen is as the
/// action would leave it. `deliverable` is asked again just before each
/// change: reading the application's windows first can take long enough for
/// the person to press Stop.
pub(super) async fn restore(
    driver: &DriverProc,
    pid: u32,
    window_id: u64,
    mode: ActDelivery,
    deliverable: &Delivery,
) -> Result<RawAct, HelperError> {
    let effect = |effect| RawAct {
        effect,
        route: None,
        submitted: None,
        submit_note: None,
        element_frame: None,
        window_frame: None,
        clipboard: None,
    };
    if !ask_back(driver, pid, window_id, mode, deliverable).await? {
        return Ok(effect(ActEffect::Confirmed));
    }
    let deadline = tokio::time::Instant::now() + RESTORE_WAIT;
    loop {
        let window = listed(driver, pid, window_id).await?;
        if window.get("is_on_screen").and_then(Value::as_bool) == Some(true) {
            return Ok(effect(ActEffect::Confirmed));
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(effect(ActEffect::Unverifiable));
        }
        tokio::time::sleep(RESTORE_POLL).await;
    }
}
