// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether the grant — a window's, a whole application's, or the entire
/// screen's — reaches `chord`, and, for a key that types a character, that
/// it is aimed at a named element.
pub(super) fn check_chord(
    chord: &Chord,
    names_element: bool,
    scope: GrantScope,
    paste_ok: bool,
) -> Result<(), ActDenied> {
    let platform = Platform::current();
    let class = match scope {
        GrantScope::Window => classify(chord, platform),
        GrantScope::App => classify_for_app(chord, platform),
        GrantScope::Screen => classify_for_screen(chord, platform),
    };
    match class {
        ChordClass::Beyond if scope == GrantScope::Window => Err(ActDenied::ChordBeyond),
        ChordClass::Beyond if scope == GrantScope::App => Err(ActDenied::DesktopChord),
        ChordClass::Beyond => Err(ActDenied::SessionChord),
        ChordClass::Paste if paste_ok => Ok(()),
        ChordClass::Paste => Err(ActDenied::Paste),
        ChordClass::Window if chord.types_text() && !names_element => Err(ActDenied::NeedsElement),
        ChordClass::Window => Ok(()),
    }
}

pub(super) fn resolve_target(
    entry: &TargetEntry,
    target: &AgentTarget,
) -> Result<DriverTarget, ActDenied> {
    Ok(match target {
        AgentTarget::Element(e) => DriverTarget::Element(resolve_element(entry, e, false)?),
        AgentTarget::Point(p) => DriverTarget::Point(resolve_point(entry, p)?),
    })
}

/// A ref, against the window's latest snapshot as the agent was given it.
/// `writes`: the action puts text into the element, which a secret field
/// never takes.
pub(super) fn resolve_element(
    entry: &TargetEntry,
    target: &ElementTarget,
    writes: bool,
) -> Result<ElementRef, ActDenied> {
    let mark = entry
        .snapshot_mark
        .as_ref()
        .ok_or(ActDenied::Stale(Staleness::NoSnapshot))?;
    if mark.generation != target.generation {
        return Err(ActDenied::Stale(Staleness::OldSnapshot));
    }
    let snapshot_id = mark
        .snapshot_id
        .clone()
        .ok_or(ActDenied::Stale(Staleness::NotActionable))?;
    if !mark.shown.contains(&target.index) {
        return Err(ActDenied::Stale(if mark.cut.contains(&target.index) {
            Staleness::CutAway(target.index)
        } else {
            Staleness::NoSuchRef(target.index)
        }));
    }
    if writes && mark.secret.contains(&target.index) {
        return Err(ActDenied::Secret);
    }
    Ok(ElementRef {
        snapshot_id,
        index: target.index,
    })
}

/// A point, in the pixels of the window's latest screenshot, mapped back to
/// the window's own pixels.
pub(super) fn resolve_point(
    entry: &TargetEntry,
    target: &PointTarget,
) -> Result<WindowPoint, ActDenied> {
    point_in(entry.capture_mark.as_ref(), target)
}

/// A point, in the pixels of the screenshot `mark` names, mapped back to the
/// pixels it was shrunk from.
pub(super) fn point_in(
    mark: Option<&CaptureMark>,
    target: &PointTarget,
) -> Result<WindowPoint, ActDenied> {
    let mark = mark.ok_or(ActDenied::Stale(Staleness::NoCapture))?;
    if mark.generation != target.generation {
        return Err(ActDenied::Stale(Staleness::OldCapture));
    }
    if !mark.full_size || mark.width == 0 || mark.height == 0 || mark.window_bounds.is_empty() {
        return Err(ActDenied::NoPointing);
    }
    let (x, y) = (target.x, target.y);
    let inside = x.is_finite()
        && y.is_finite()
        && x >= 0.0
        && y >= 0.0
        && x < f64::from(mark.width)
        && y < f64::from(mark.height);
    if !inside {
        return Err(ActDenied::OutOfImage);
    }
    Ok(WindowPoint {
        x: x * f64::from(mark.native_width) / f64::from(mark.width),
        y: y * f64::from(mark.native_height) / f64::from(mark.height),
        window_width: mark.window_bounds.width,
        window_height: mark.window_bounds.height,
    })
}
