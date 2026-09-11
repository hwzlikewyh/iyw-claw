const PRIVATE_ARTIFACT_HEADING: &str = "## Current-turn final artifact delivery";

pub(crate) fn is_private_title_candidate(title: &str) -> bool {
    let title = title.trim_start();
    title.starts_with(PRIVATE_ARTIFACT_HEADING)
        || title.starts_with(crate::user_memory::USER_CONTEXT_START)
}
