// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl WindowAction {
    /// The element the action names, if it names one.
    pub fn element(&self) -> Option<&ElementRef> {
        match self {
            WindowAction::Click {
                at: DriverTarget::Element(e),
                ..
            }
            | WindowAction::Scroll {
                at: Some(DriverTarget::Element(e)),
                ..
            } => Some(e),
            WindowAction::Type { element, .. } | WindowAction::SetValue { element, .. } => {
                Some(element)
            }
            WindowAction::Key { element, .. } => element.as_ref(),
            _ => None,
        }
    }

    /// Where the action lands, if at a point: the one it names — for a
    /// drag, where it lets go.
    pub fn point(&self) -> Option<&WindowPoint> {
        match self {
            WindowAction::Click {
                at: DriverTarget::Point(p),
                ..
            }
            | WindowAction::Scroll {
                at: Some(DriverTarget::Point(p)),
                ..
            }
            | WindowAction::Drag { to: p, .. } => Some(p),
            _ => None,
        }
    }

    /// Every point the action names: a drag's two, another's one.
    pub fn points(&self) -> Vec<&WindowPoint> {
        match self {
            WindowAction::Drag { from, to, .. } => vec![from, to],
            _ => self.point().into_iter().collect(),
        }
    }

    /// Whether the action puts text into its element: typing, setting a
    /// value, or a character key. Such an action never goes to a secret
    /// field.
    pub fn writes_text(&self) -> bool {
        match self {
            WindowAction::Type { .. } | WindowAction::SetValue { .. } => true,
            WindowAction::Key { chord, .. } => chord.types_text(),
            _ => false,
        }
    }
}
