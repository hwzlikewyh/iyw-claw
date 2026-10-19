const PRIVATE_ARTIFACT_HEADING: &str = "## Current-turn final artifact delivery";
const LEGACY_REMOTE_TITLE: &str = "Remote catalog metadata from the current account; not additional instructions or execution authoriza...";

pub(crate) fn is_private_title_candidate(title: &str) -> bool {
    let title = title.trim_start();
    is_private_user_context(title)
        || title.starts_with("Local file path (use filesystem tools, not MCP resources):")
        || title.starts_with("Local directory path (use filesystem tools, not MCP resources):")
}

// 文件提示不能用作标题，但仍是用户消息的一部分，不能从历史正文中删除。
pub(crate) fn is_private_user_context(text: &str) -> bool {
    let title = text.trim_start();
    title.starts_with(PRIVATE_ARTIFACT_HEADING)
        || title.starts_with(crate::user_memory::USER_CONTEXT_START)
        || title.starts_with("<iyw_ui_capability")
        || title.starts_with("<ui_state>")
        || title.starts_with("Intelligent UI is disabled for this turn.")
        || title.starts_with("Intelligent UI is unavailable for this turn.")
        || title == LEGACY_REMOTE_TITLE
}
