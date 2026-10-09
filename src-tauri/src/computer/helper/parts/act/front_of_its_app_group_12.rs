// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A drag at the front on macOS goes in as real pointer input once the
/// driver has brought the application forward — the application, not the
/// window: the windows the application names as its focused and main ones
/// come forward with it, and the drag lands on whatever is then under the
/// pointer. So it is sent only for the window that is already its
/// application's front one: on the screen; ahead, in the window server's
/// order, of every other window of the application on this desktop or on
/// another (that one would come forward, and the desktop switch to it); and
/// not behind another the application names as focused or main. Asked just
/// before the drag.
#[cfg(target_os = "macos")]
pub(super) async fn front_of_its_app(
    driver: &DriverProc,
    pid: u32,
    window_id: u64,
) -> Result<(), HelperError> {
    let result = driver
        .call(
            "list_windows",
            json!({ "pid": pid, "on_screen_only": false }),
            MEASURE_TIMEOUT,
        )
        .await?;
    if result.is_error {
        return Err(crate::computer::helper::ops::tool_error(
            "list_windows",
            &result,
        ));
    }
    let windows = crate::computer::helper::ops::parse_windows(
        crate::computer::helper::ops::structured("list_windows", &result)?,
    )?;
    let Some(target) = windows
        .iter()
        .find(|w| w.pid == pid && w.window_id == window_id)
    else {
        return Err(HelperError::new(
            HelperErrorCode::NoSuchWindow,
            "the window is gone",
        ));
    };
    if !target.on_screen {
        return Err(HelperError::new(
            HelperErrorCode::Occluded,
            "The window is not on the screen — minimized, hidden or on another desktop (Space) — \
             and a drag at the front goes wherever the pointer is. computer_restore brings back a \
             minimized window or a hidden application; otherwise ask the user to bring it back.",
        ));
    }
    let front = target.z_index;
    let ahead = windows
        .iter()
        .filter(|w| w.pid == pid && w.window_id != window_id)
        .filter(|w| w.on_screen || w.on_current_space == Some(false))
        .any(|w| match (w.z_index, front) {
            (Some(other), Some(front)) => other > front,
            // Where the order cannot be told, it cannot be ruled out.
            _ => true,
        });
    let (focused, main) = crate::computer::helper::axwin::focused_and_main(pid).await;
    let named_other = [focused, main]
        .into_iter()
        .flatten()
        .any(|named| named != window_id);
    if ahead || named_other {
        return Err(HelperError::new(
            HelperErrorCode::Occluded,
            "Another window of this application is in front of this one, or is the one the \
             application brings forward, and a drag at the front would land on it — so nothing \
             was sent. Once this window is the application's front one, drag again: ask the user \
             to click it, or bring it forward with a computer_click on it at the front.",
        ));
    }
    Ok(())
}

/// Whether a menu command an application shared as a whole may be chosen
/// (macOS): not in the Apple menu or the application menu, which reach past
/// the application — restarting, logging out, Services, hiding every other
/// application — and not one whose shortcut is ⌘V, a paste by another name.
/// A command the walk through the menus as they stand cannot reach — a title
/// missing or met twice, a menu that fills itself only once opened — is
/// refused: what it is cannot be told before it is chosen.
#[cfg(target_os = "macos")]
pub(super) async fn menu_in_reach(
    pid: u32,
    path: &[String],
    paste_ok: bool,
) -> Result<(), HelperError> {
    use crate::computer::helper::axwin::{menu_target, MENU_NO_COMMAND};
    let target = menu_target(pid, path.to_vec()).await;
    match target.bar_index {
        None => Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            format!(
                "The menu bar has no single menu titled \"{}\". Read the menus again \
                 (computer_snapshot) and use their titles exactly.",
                path.first().map(String::as_str).unwrap_or_default()
            ),
        )),
        Some(0 | 1) => Err(HelperError::new(
            HelperErrorCode::BeyondApp,
            "The Apple menu and the application menu reach past the application — restarting, \
             logging out, Services, hiding the others — so nothing in them is chosen for an agent. \
             The application's own shortcuts do what it needs of them (⌘, for its settings, ⌘Q to \
             quit it).",
        )),
        Some(_) if !target.reached => Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            "That command cannot be found in the application's menus as they stand: a title is \
             missing or appears twice, or the menu fills itself only when opened. Read the menus \
             again (computer_snapshot) and use their titles exactly; for a menu that fills itself, \
             click it open and choose the item by ref.",
        )),
        Some(_) => match target.shortcut {
            Some((key, mask))
                if !paste_ok && key.eq_ignore_ascii_case("v") && mask & MENU_NO_COMMAND == 0 =>
            {
                Err(HelperError::new(
                    HelperErrorCode::PasteRefused,
                    "That command pastes (⌘V), and the clipboard is the user's own: what is on it \
                     may not come from any window you may read. Type the text with computer_type \
                     instead.",
                ))
            }
            _ => Ok(()),
        },
    }
}
