const RECALL_INDEX_ENV: &str = "IYW_CLAW_USER_MEMORY_RECALL_INDEX";
const RECALL_TOOL_ENV: &str = "IYW_CLAW_USER_MEMORY_RECALL_TOOL";

pub(super) fn configured_recall_index_enabled() -> bool {
    configured_enabled(RECALL_INDEX_ENV)
}

pub(super) fn configured_recall_tool_enabled() -> bool {
    configured_enabled(RECALL_TOOL_ENV)
}

fn configured_enabled(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .as_deref()
        .map(parse_enabled)
        .unwrap_or(true)
}

fn parse_enabled(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}
