const CAPABILITY_START: &str = "<iyw_ui_capability version=\"2\">";
const CAPABILITY_END: &str = "</iyw_ui_capability>";
const STATE_START: &str = "<ui_state>";
const STATE_END: &str = "</ui_state>";
const NOTICES: [&str; 2] = [
    "Intelligent UI is disabled for this turn. Reply using ordinary Markdown. Do not emit iyw-ui or iyw-widget fences.",
    "Intelligent UI is unavailable for this turn. Use ordinary Markdown; do not emit iyw-ui or iyw-widget fences.",
];

/// 旧版 UI 注入没有私有标记，仅清理前缀中的完整宿主能力说明。
pub(super) fn strip_legacy_ui_context(input: &str) -> &str {
    let mut visible = input;
    loop {
        let trimmed = visible.trim_start();
        let remainder = if let Some(body) = trimmed.strip_prefix(CAPABILITY_START) {
            let Some((guide, tail)) = body.split_once(CAPABILITY_END) else {
                return visible;
            };
            if !guide.trim_start().starts_with("---")
                || !guide
                    .lines()
                    .take(3)
                    .any(|line| line.trim() == "name: ui-render")
            {
                return visible;
            }
            tail
        } else if let Some(tail) = NOTICES.iter().find_map(|notice| {
            trimmed
                .strip_prefix(*notice)
                .filter(|tail| tail.is_empty() || tail.starts_with(char::is_whitespace))
        }) {
            tail
        } else {
            return visible;
        };
        visible = remainder.trim_start();
        if let Some(state) = visible.strip_prefix(STATE_START) {
            let Some((_, tail)) = state.split_once(STATE_END) else {
                return visible;
            };
            visible = tail.trim_start();
        }
    }
}
