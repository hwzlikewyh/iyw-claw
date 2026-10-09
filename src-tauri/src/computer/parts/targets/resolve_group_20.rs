// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The action as the helper carries it out: keys judged for a window grant,
/// refs and points resolved against what the agent last read of the window.
/// `paste_ok`: the clipboard holds what an agent put there itself, which a
/// paste may then write into the window (see `commands::computer`).
pub(super) fn resolve(
    entry: &TargetEntry,
    request: &ComputerActRequest,
    paste_ok: bool,
) -> Result<WindowAction, ActDenied> {
    let scope = entry
        .grant
        .as_ref()
        .map_or(GrantScope::Window, |grant| grant.scope);
    Ok(match request {
        ComputerActRequest::Click {
            target,
            button,
            count,
            modifiers,
        } => {
            check_pointer_modifiers(*modifiers, scope)?;
            if *count == 2 && !modifiers.is_empty() {
                return Err(ActDenied::DoubleClickModifiers);
            }
            WindowAction::Click {
                at: resolve_target(entry, target)?,
                button: *button,
                count: *count,
                modifiers: *modifiers,
            }
        }
        ComputerActRequest::Drag {
            from,
            to,
            button,
            modifiers,
            duration_ms,
        } => {
            check_pointer_modifiers(*modifiers, scope)?;
            if !crate::computer::keys::drag_carries_modifiers(*modifiers, Platform::current()) {
                return Err(ActDenied::DragModifiers);
            }
            WindowAction::Drag {
                from: resolve_point(entry, from)?,
                to: resolve_point(entry, to)?,
                button: *button,
                modifiers: *modifiers,
                duration_ms: duration_ms
                    .unwrap_or(DEFAULT_DRAG_MS)
                    .min(crate::computer::types::MAX_DRAG_MS),
            }
        }
        ComputerActRequest::Scroll {
            target,
            direction,
            amount,
            unit,
        } => WindowAction::Scroll {
            at: target
                .as_ref()
                .map(|t| resolve_target(entry, t))
                .transpose()?,
            direction: *direction,
            amount: *amount,
            unit: *unit,
        },
        ComputerActRequest::Type {
            target,
            text,
            submit,
        } => WindowAction::Type {
            element: resolve_element(entry, target, true)?,
            text: text.clone(),
            submit: *submit,
        },
        ComputerActRequest::Key { target, chord, .. } => {
            check_chord(chord, target.is_some(), scope, paste_ok)?;
            WindowAction::Key {
                element: target
                    .as_ref()
                    .map(|t| resolve_element(entry, t, chord.types_text()))
                    .transpose()?,
                chord: *chord,
            }
        }
        // A held key is the key pressed, again and again: each press is an
        // action of its own (see `commands::computer`).
        ComputerActRequest::HoldKey { target, chord, .. } => {
            check_chord(chord, target.is_some(), scope, paste_ok)?;
            WindowAction::Key {
                element: target
                    .as_ref()
                    .map(|t| resolve_element(entry, t, chord.types_text()))
                    .transpose()?,
                chord: *chord,
            }
        }
        ComputerActRequest::SetValue { target, value } => WindowAction::SetValue {
            element: resolve_element(entry, target, true)?,
            value: value.clone(),
        },
        ComputerActRequest::Restore => WindowAction::Restore,
        ComputerActRequest::InvokeMenu { path } => {
            if scope == GrantScope::Window {
                return Err(ActDenied::AppGrantRequired);
            }
            if Platform::current() == Platform::Windows {
                return Err(ActDenied::MenusUnavailable);
            }
            // A paste by its menu is a paste: it writes the person's
            // clipboard into the window ("Paste Special", "Unformatted Text"
            // included). The helper checks the command's own shortcut too.
            if !paste_ok
                && path
                    .iter()
                    .any(|title| crate::computer::keys::names_paste(title))
            {
                return Err(ActDenied::Paste);
            }
            WindowAction::InvokeMenu {
                path: path.iter().map(|title| title.trim().to_string()).collect(),
            }
        }
        ComputerActRequest::SetFrame {
            x,
            y,
            width,
            height,
        } => {
            check_frame(*x, *y, *width, *height)?;
            WindowAction::SetFrame {
                x: *x,
                y: *y,
                width: *width,
                height: *height,
            }
        }
    })
}

/// Whether what is given of a window's frame could be one: every number
/// finite, neither side under [`MIN_WINDOW_SIDE`], nothing beyond
/// [`MAX_WINDOW_EXTENT`]. What is left out the helper takes from the window as
/// it finds it just before, not from a listing that may be out of date.
pub(super) fn check_frame(
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<(), ActDenied> {
    use crate::computer::types::{MAX_WINDOW_EXTENT, MIN_WINDOW_SIDE};
    let place = |n: f64| n.is_finite() && n.abs() <= MAX_WINDOW_EXTENT;
    let side = |n: f64| n.is_finite() && (MIN_WINDOW_SIDE..=MAX_WINDOW_EXTENT).contains(&n);
    let given = x.is_some() || y.is_some() || width.is_some() || height.is_some();
    let fits = x.is_none_or(place)
        && y.is_none_or(place)
        && width.is_none_or(side)
        && height.is_none_or(side);
    if given && fits {
        Ok(())
    } else {
        Err(ActDenied::BadFrame)
    }
}

/// How long a drag's path takes when the agent does not say: the driver's
/// own default.
pub(super) const DEFAULT_DRAG_MS: u32 = 500;

/// An action on the entire screen as the helper carries it out: a click, a
/// drag or a scroll, every point resolved against the screen's latest
/// picture (`mark`) as the agent was given it. Anything else goes to a
/// window.
pub(super) fn resolve_on_screen(
    mark: Option<&CaptureMark>,
    request: &ComputerActRequest,
) -> Result<WindowAction, ActDenied> {
    Ok(match request {
        ComputerActRequest::Click {
            target: AgentTarget::Point(point),
            button,
            count,
            modifiers,
        } => {
            check_pointer_modifiers(*modifiers, GrantScope::Screen)?;
            WindowAction::Click {
                at: DriverTarget::Point(point_in(mark, point)?),
                button: *button,
                count: *count,
                modifiers: *modifiers,
            }
        }
        ComputerActRequest::Drag {
            from,
            to,
            button,
            modifiers,
            duration_ms,
        } => {
            check_pointer_modifiers(*modifiers, GrantScope::Screen)?;
            if !crate::computer::keys::drag_carries_modifiers(*modifiers, Platform::current()) {
                return Err(ActDenied::DragModifiers);
            }
            WindowAction::Drag {
                from: point_in(mark, from)?,
                to: point_in(mark, to)?,
                button: *button,
                modifiers: *modifiers,
                duration_ms: duration_ms
                    .unwrap_or(DEFAULT_DRAG_MS)
                    .min(crate::computer::types::MAX_DRAG_MS),
            }
        }
        ComputerActRequest::Scroll {
            target: Some(AgentTarget::Point(point)),
            direction,
            amount,
            unit,
        } => WindowAction::Scroll {
            at: Some(DriverTarget::Point(point_in(mark, point)?)),
            direction: *direction,
            amount: *amount,
            unit: *unit,
        },
        _ => return Err(ActDenied::ScreenPointerOnly),
    })
}

/// Whether the grant reaches `modifiers` held over a click or a drag: a
/// window's (see `keys::pointer_modifiers_allowed`), a whole application's
/// (`keys::pointer_modifiers_allowed_for_app`), or the entire screen's —
/// which reaches every one.
pub(super) fn check_pointer_modifiers(
    modifiers: crate::computer::keys::Modifiers,
    scope: GrantScope,
) -> Result<(), ActDenied> {
    let platform = Platform::current();
    let allowed = match scope {
        GrantScope::Window => crate::computer::keys::pointer_modifiers_allowed(modifiers, platform),
        GrantScope::App => {
            crate::computer::keys::pointer_modifiers_allowed_for_app(modifiers, platform)
        }
        GrantScope::Screen => true,
    };
    match (allowed, scope) {
        (true, _) => Ok(()),
        (false, GrantScope::Window) => Err(ActDenied::ChordBeyond),
        (false, _) => Err(ActDenied::DesktopChord),
    }
}
