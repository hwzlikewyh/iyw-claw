use super::memory_policy_required;

#[test]
fn policy_capability_is_allowed_before_preflight() {
    assert!(!memory_policy_required(
        "iyw.memory.policy.read.v1",
        Some(7),
        0,
    ));
}

#[test]
fn other_memory_capabilities_require_matching_loaded_turn() {
    assert!(memory_policy_required(
        "iyw.memory.recall.search.v1",
        Some(7),
        0,
    ));
    assert!(!memory_policy_required(
        "iyw.memory.recall.search.v1",
        Some(7),
        7,
    ));
    assert!(memory_policy_required(
        "iyw.memory.recall.search.v1",
        None,
        7,
    ));
}

#[test]
fn non_memory_capabilities_are_not_policy_gated() {
    assert!(!memory_policy_required("iyw.browser.tabs.list.v1", None, 0));
}
