// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The titles of the first two items of `pid`'s menu bar — the Apple menu
/// and the application menu — as the menu bar has them now; `None` when it
/// would not say, or gave one a tree row could not be matched by: empty
/// (the driver writes no title then), or with a quote or a line break in it.
pub async fn protected_menu_titles(pid: u32) -> Option<[String; 2]> {
    tokio::task::spawn_blocking(move || {
        let app = application(pid)?;
        let bar = attribute(&app, "AXMenuBar").ok()?;
        bound(&bar);
        let items = elements(&bar, "AXChildren").ok()?;
        let title = |item: &CFType| {
            attribute(item, "AXTitle")
                .ok()
                .and_then(|v| v.downcast_into::<CFString>())
                .map(|t| t.to_string().trim().to_string())
                .filter(|t| !t.is_empty() && !t.contains(['"', '\n', '\r']))
        };
        Some([title(items.first()?)?, title(items.get(1)?)?])
    })
    .await
    .ok()
    .flatten()
}

/// The mask bit that says a menu shortcut is pressed without ⌘
/// (`kAXMenuItemModifierNoCommand`).
pub const MENU_NO_COMMAND: i64 = 8;

/// Follow `path` through `pid`'s menus by title, as the driver will to
/// choose it — without choosing anything: AppKit lists a closed menu's items
/// too. A title met more than once, or not at all, stops the walk there.
pub async fn menu_target(pid: u32, path: Vec<String>) -> MenuTarget {
    tokio::task::spawn_blocking(move || menu_target_now(pid, &path))
        .await
        .unwrap_or_default()
}
