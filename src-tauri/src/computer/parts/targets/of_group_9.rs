// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Aim {
    /// Where `action` lands, from what the agent last read of the window:
    /// for a point, the screenshot's pixels scaled to the window's units.
    pub(in crate::computer::targets) fn of(entry: &TargetEntry, action: &WindowAction) -> Aim {
        if action.element().is_some() {
            return Aim::Element;
        }
        match (action.point(), entry.capture_mark.as_ref()) {
            (Some(point), Some(mark)) if mark.native_width > 0 && mark.native_height > 0 => {
                Aim::Offset {
                    x: point.x * point.window_width / f64::from(mark.native_width),
                    y: point.y * point.window_height / f64::from(mark.native_height),
                }
            }
            _ => Aim::Focus,
        }
    }

    /// The point on the screen, in desktop units, from what the helper
    /// reported of the action: the middle of the element's frame, or the
    /// offset from where the window was measured to be.
    pub fn landing(&self, act: &RawAct) -> Option<(f64, f64)> {
        let placed = |r: &Rect| !r.is_empty() && r.x.is_finite() && r.y.is_finite();
        match *self {
            Aim::Focus => None,
            Aim::Element => act
                .element_frame
                .filter(placed)
                .map(|f| (f.x + f.width / 2.0, f.y + f.height / 2.0)),
            Aim::Offset { x, y } => act.window_frame.filter(placed).map(|w| (w.x + x, w.y + y)),
        }
    }
}
