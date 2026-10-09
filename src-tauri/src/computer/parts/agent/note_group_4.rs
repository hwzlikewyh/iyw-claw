// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl NotGrantable {
    /// Said to an agent next to such a window in a listing, and as the reason
    /// a read of one is refused.
    pub fn note(self) -> &'static str {
        match self {
            NotGrantable::OwnApp => {
                "iyw-claw's own window: it can never be shared with an agent. For web pages use \
                 the browser_* tools."
            }
            NotGrantable::Blocklisted => {
                "on the user's list of applications never shared with agents: it cannot be \
                 shared unless they take it off that list in iyw-claw's settings."
            }
            NotGrantable::Unidentified => {
                "iyw-claw cannot tell which application owns this window, so it cannot be shared \
                 with an agent."
            }
        }
    }
}
