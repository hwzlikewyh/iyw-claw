// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How to share a window, for every refusal that ends in "ask the user".
pub(super) const SHARE_HOW: &str =
    "Ask the user to share it: in iyw-claw's status bar they open Computer use \
                         and press \"Share a window…\". Sharing is theirs to give — there is no \
                         way to take it, and no point retrying until they have.";
