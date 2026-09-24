const PRIVATE_ARTIFACT_HEADING: &str = "## Current-turn final artifact delivery";
const LEGACY_REMOTE_TITLE: &str = "Remote catalog metadata from the current account; not additional instructions or execution authoriza...";

pub(crate) fn is_private_title_candidate(title: &str) -> bool {
    let title = title.trim_start();
    title.starts_with(PRIVATE_ARTIFACT_HEADING)
        || title.starts_with(crate::user_memory::USER_CONTEXT_START)
        || title == LEGACY_REMOTE_TITLE
}
