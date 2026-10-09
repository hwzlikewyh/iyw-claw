// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) fn menu_target_now(pid: u32, path: &[String]) -> MenuTarget {
    let deadline = std::time::Instant::now() + MENU_WALK;
    let mut out = MenuTarget::default();
    let Some(app) = application(pid) else {
        return out;
    };
    let Ok(bar) = attribute(&app, "AXMenuBar") else {
        return out;
    };
    bound(&bar);
    let Some((first, rest)) = path.split_first() else {
        return out;
    };
    let items = elements(&bar, "AXChildren").unwrap_or_default();
    let Some(index) = only_titled(&items, first, deadline) else {
        return out;
    };
    out.bar_index = Some(index);
    let mut current = items[index].clone();
    for title in rest {
        let children = menu_children(&current, deadline);
        let Some(next) = only_titled(&children, title, deadline) else {
            return out;
        };
        current = children[next].clone();
    }
    if std::time::Instant::now() >= deadline {
        return out;
    }
    out.reached = true;
    let key = attribute(&current, "AXMenuItemCmdChar")
        .ok()
        .and_then(|v| v.downcast_into::<CFString>())
        .map(|key| key.to_string())
        .filter(|key| !key.trim().is_empty());
    if let Some(key) = key {
        let mask = attribute(&current, "AXMenuItemCmdModifiers")
            .ok()
            .and_then(|v| v.downcast_into::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(0);
        out.shortcut = Some((key, mask));
    }
    out
}

/// A menu item's items, the untitled menu AppKit puts between them seen
/// through, as the driver sees them — as many as `deadline` leaves time for.
pub(super) fn menu_children(item: &CFType, deadline: std::time::Instant) -> Vec<CFType> {
    let mut out = Vec::new();
    for child in elements(item, "AXChildren").unwrap_or_default() {
        if std::time::Instant::now() >= deadline {
            break;
        }
        let role = attribute(&child, "AXRole")
            .ok()
            .and_then(|v| v.downcast_into::<CFString>())
            .map(|r| r.to_string());
        if role.as_deref() == Some("AXMenu") {
            out.extend(elements(&child, "AXChildren").unwrap_or_default());
        } else {
            out.push(child);
        }
    }
    out
}

/// Where among `items` the one titled `title` is — trimmed, as the driver
/// matches — when exactly one is, and `deadline` left time to look at them
/// all.
pub(super) fn only_titled(
    items: &[CFType],
    title: &str,
    deadline: std::time::Instant,
) -> Option<usize> {
    let wanted = title.trim();
    let mut found = None;
    for (index, item) in items.iter().enumerate() {
        if std::time::Instant::now() >= deadline {
            return None;
        }
        let matches = attribute(item, "AXTitle")
            .ok()
            .and_then(|v| v.downcast_into::<CFString>())
            .is_some_and(|t| t.to_string().trim() == wanted);
        if matches {
            if found.is_some() {
                return None;
            }
            found = Some(index);
        }
    }
    found
}
