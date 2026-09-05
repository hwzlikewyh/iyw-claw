use std::time::Duration;

use serde_json::json;

use super::agent_browser::{BrowserRoute, BrowserRouteProvider};
use super::agent_browser_handoff_state::captured_object;
use super::agent_browser_input::{
    legacy_input, managed_input, managed_semantic_command, requires_managed,
};
use super::agent_browser_request::opencli_request;
use super::agent_browser_route::{ensure_provider_matches_input, validate_opencli_tab_session};
use super::opencli::{is_supported_advanced_command, OpencliFailureKind};
use super::opencli_failure::parse_execution;
use super::types::BrowserAgentIdentity;
use crate::commands::internet_tools::OpencliExecution;

#[test]
fn legacy_tools_normalize_to_unified_actions() {
    let input = legacy_input("browser_command", &json!({ "command": "get" })).unwrap();

    assert_eq!(input["action"], "advanced");
    assert_eq!(input["command"], "get");
}

#[test]
fn only_explicit_window_and_user_actions_require_managed() {
    assert!(requires_managed("request_user_action"));
    assert!(requires_managed("present"));
    assert!(requires_managed("close_window"));
    assert!(!requires_managed("snapshot"));
    assert!(!requires_managed("read"));
    assert!(!requires_managed("scroll"));
}

#[test]
fn new_tab_and_full_page_aliases_match_opencli_contract() {
    let opened = opencli_request(
        "open",
        &json!({ "url": "https://example.com", "newTab": true }),
        None,
    )
    .unwrap();
    assert_eq!(opened.command, "tab");
    assert_eq!(opened.args, ["new", "https://example.com"]);
    assert_eq!(opened.target, None);

    let screenshot = opencli_request("screenshot", &json!({ "fullPage": true }), None).unwrap();
    assert_eq!(screenshot.command, "screenshot");
    assert_eq!(screenshot.args, ["--full"]);
}

#[test]
fn opencli_opaque_tab_ids_do_not_treat_session_as_target() {
    let session_only =
        opencli_request("snapshot", &json!({ "tab_id": "opencli:iyw-test" }), None).unwrap();
    assert_eq!(session_only.target, None);

    let exact = opencli_request(
        "snapshot",
        &json!({ "tab_id": "opencli:iyw-test:page-42" }),
        None,
    )
    .unwrap();
    assert_eq!(exact.target.as_deref(), Some("page-42"));
}

#[test]
fn raw_read_keeps_selector_filter() {
    let request = opencli_request(
        "read",
        &json!({ "raw": true, "filter": "main article" }),
        None,
    )
    .unwrap();

    assert_eq!(request.command, "get");
    assert_eq!(request.args, ["html", "--selector", "main article"]);
}

#[test]
fn unsupported_opencli_snapshot_options_fail_instead_of_switching() {
    let error = opencli_request("snapshot", &json!({ "depth": 3 }), None).unwrap_err();

    assert!(error.message.contains("does not support"));
}

#[test]
fn managed_semantic_targets_use_the_existing_find_command() {
    let input = managed_input(
        "fill",
        &json!({
            "tabId": "managed-tab",
            "target": { "role": "textbox", "name": "Email" },
            "text": "user@example.com"
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
            "user@example.com",
            "--name",
            "Email"
        ])
    );
}

#[test]
fn advanced_opencli_arguments_cannot_override_the_pinned_tab() {
    let error = opencli_request(
        "advanced",
        &json!({ "command": "get", "arguments": ["url", "--tab=other"] }),
        None,
    )
    .unwrap_err();

    assert!(error.message.contains("Unsafe"));
}

#[test]
fn advanced_opencli_commands_exclude_session_and_tab_control() {
    assert!(is_supported_advanced_command("eval"));
    assert!(is_supported_advanced_command("network"));
    assert!(is_supported_advanced_command("dialog"));
    assert!(!is_supported_advanced_command("open"));
    assert!(!is_supported_advanced_command("tab"));
    assert!(!is_supported_advanced_command("close"));
    assert!(!is_supported_advanced_command("bind"));
}

#[test]
fn provider_lock_rejects_cross_provider_tab_ids() {
    let opencli = BrowserRoute {
        provider: BrowserRouteProvider::Opencli {
            session: "iyw-test".to_string(),
            target: None,
        },
    };
    assert!(
        ensure_provider_matches_input(Some(&opencli), &json!({ "tab_id": "managed-tab" })).is_err()
    );

    let managed = BrowserRoute {
        provider: BrowserRouteProvider::Managed { reason: None },
    };
    assert!(ensure_provider_matches_input(
        Some(&managed),
        &json!({ "tab_id": "opencli:iyw-test:page-1" })
    )
    .is_err());
}

#[test]
fn opencli_tab_ids_are_scoped_to_the_agent_session() {
    let identity = BrowserAgentIdentity {
        connection_id: "connection-a".to_string(),
        conversation_id: Some(42),
        turn_generation: 1,
    };
    assert!(validate_opencli_tab_session(
        &identity,
        &json!({ "tab_id": "opencli:another-session:page-1" })
    )
    .is_err());
}

#[test]
fn opencli_exit_codes_drive_handoff_policy() {
    let auth = parse_execution(&execution(77, "", "Authentication required")).unwrap_err();
    assert_eq!(auth.kind, OpencliFailureKind::UserAction);

    let bridge = parse_execution(&execution(69, "", "Browser unavailable")).unwrap_err();
    assert_eq!(bridge.kind, OpencliFailureKind::BridgeUnavailable);

    let timeout = parse_execution(&execution(75, "", "Command failed")).unwrap_err();
    assert_eq!(timeout.kind, OpencliFailureKind::Timeout);
}

#[test]
fn selector_errors_never_become_user_action_handoffs() {
    let failure = parse_execution(&OpencliExecution {
        success: false,
        exit_code: Some(1),
        stdout: json!({
            "error": {
                "code": "selector_not_found",
                "message": "Please log in after fixing the selector"
            }
        })
        .to_string(),
        stderr: String::new(),
    })
    .unwrap_err();

    assert_eq!(failure.kind, OpencliFailureKind::Selector);
}

#[test]
fn auth_capture_accepts_direct_and_wrapped_eval_results() {
    let direct = json!({ "cookies": "sid=1", "localStorage": {} });
    assert_eq!(
        captured_object(&direct)
            .and_then(|value| value.get("cookies"))
            .and_then(serde_json::Value::as_str),
        Some("sid=1")
    );

    let wrapped = json!({ "value": { "cookies": "sid=2", "sessionStorage": {} } });
    assert_eq!(
        captured_object(&wrapped)
            .and_then(|value| value.get("cookies"))
            .and_then(serde_json::Value::as_str),
        Some("sid=2")
    );
}

fn execution(exit_code: i32, stdout: &str, stderr: &str) -> OpencliExecution {
    OpencliExecution {
        success: false,
        exit_code: Some(exit_code),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

#[test]
fn requested_timeout_is_bounded() {
    let request = opencli_request("snapshot", &json!({ "timeoutMs": 300_000 }), None).unwrap();

    assert_eq!(request.timeout, Duration::from_secs(300));
}
