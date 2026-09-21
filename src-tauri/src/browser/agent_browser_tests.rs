use super::agent_browser_input::{legacy_input, managed_input, managed_semantic_command};
use serde_json::json;

#[test]
fn legacy_tools_normalize_to_unified_actions() {
    let input = legacy_input("browser_command", &json!({ "command": "get" })).unwrap();
    assert_eq!(input["action"], "advanced");
    assert_eq!(input["command"], "get");
}

#[test]
fn retired_external_tab_ids_are_rejected_for_both_aliases() {
    for field in ["tab_id", "tabId"] {
        let input = json!({(field): "opencli:iyw-test:page-42"});
        let error = managed_input("snapshot", &input).unwrap_err();
        assert!(error.message.contains("retired"));
    }
}

#[test]
fn managed_tab_and_capture_aliases_are_preserved() {
    let input = managed_input(
        "screenshot",
        &json!({
            "tabId":"managed-tab", "fullPage":true, "newTab":false, "timeoutMs":5000
        }),
    )
    .unwrap();
    assert_eq!(input["tab_id"], "managed-tab");
    assert_eq!(input["full_page"], true);
    assert_eq!(input["new_tab"], false);
    assert_eq!(input["timeout_ms"], 5000);
}

#[test]
fn managed_semantic_targets_use_the_existing_find_command() {
    let input = managed_input(
        "fill",
        &json!({
            "tabId":"managed-tab", "target":{"role":"textbox","name":"Email"},
            "text":"test@example.com"
        }),
    )
    .unwrap();
    let command = managed_semantic_command("fill", &input).unwrap().unwrap();
    assert_eq!(command["tab_id"], "managed-tab");
    assert_eq!(command["command"], "find");
    assert_eq!(
        command["arguments"],
        json!([
            "role",
            "textbox",
            "fill",
            "test@example.com",
            "--name",
            "Email"
        ])
    );
}

#[test]
fn advanced_command_requires_an_explicit_command() {
    assert!(managed_input("advanced", &json!({})).is_err());
}

#[tokio::test]
async fn disabled_browser_rejects_all_actions_without_external_fallback() {
    use super::agent_tool_cancellation::AgentToolContext;
    use super::{BrowserAgentIdentity, BrowserCapability, BrowserErrorCode, BrowserSessionManager};
    use std::sync::atomic::Ordering;
    let manager = BrowserSessionManager::new(BrowserCapability::unsupported("no runtime"));
    manager
        .managed_browser_enabled
        .store(false, Ordering::Release);
    let identity = BrowserAgentIdentity {
        connection_id: "test-connection".into(),
        conversation_id: None,
        turn_generation: 1,
    };
    let cancellation = tokio_util::sync::CancellationToken::new();
    for action in [
        "list_tabs",
        "open",
        "snapshot",
        "read",
        "click",
        "fill",
        "press",
        "scroll",
        "wait",
        "screenshot",
        "advanced",
        "present",
        "close_window",
        "close_tab",
        "request_user_action",
    ] {
        let context = AgentToolContext {
            identity: &identity,
            cancellation: &cancellation,
        };
        let error = manager
            .agent_browser(context, &json!({"action":action}))
            .await
            .unwrap_err();
        assert_eq!(error.code, BrowserErrorCode::BrowserControlChanged);
    }
}
