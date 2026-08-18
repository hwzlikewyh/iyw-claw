use super::*;

const AES_KEY: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";

fn config(external_base_url: &str) -> WecomAgentConfig {
    WecomAgentConfig {
        corp_id: "ww-test".to_string(),
        agent_id: "1000002".to_string(),
        callback_path: "0123456789abcdef".to_string(),
        external_base_url: external_base_url.to_string(),
        setup_state: "pending_callback".to_string(),
        callback_verified_at: None,
        default_user_id: String::new(),
    }
}

fn secrets() -> WecomAgentSecrets {
    WecomAgentSecrets {
        version: 1,
        app_secret: "secret".to_string(),
        callback_token: "token".to_string(),
        encoding_aes_key: AES_KEY.to_string(),
    }
}

#[test]
fn accepts_public_https_base_url() {
    assert!(validate_config(&config("https://example.com/claw"), &secrets()).is_ok());
}

#[test]
fn rejects_external_url_metadata() {
    for value in [
        "http://example.com",
        "https://user:pass@example.com",
        "https://example.com?token=secret",
        "https://example.com#callback",
    ] {
        assert!(
            validate_config(&config(value), &secrets()).is_err(),
            "{value}"
        );
    }
}

#[test]
fn requires_verified_callback_before_ready() {
    let ready = serde_json::json!({
        "setup_state": "ready",
        "callback_verified_at": "2026-08-17T08:00:00Z",
    });
    assert!(ensure_ready_config(&ready).is_ok());

    for incomplete in [
        serde_json::json!({ "setup_state": "pending_callback" }),
        serde_json::json!({ "setup_state": "ready" }),
        serde_json::json!({
            "setup_state": "ready",
            "callback_verified_at": "   ",
        }),
    ] {
        assert!(ensure_ready_config(&incomplete).is_err());
    }
}

#[test]
fn new_config_cannot_claim_callback_verification() {
    let prepared = prepare_new_config(
        r#"{
            "setup_state": "ready",
            "callback_verified_at": "forged",
            "corp_id": "ww-test"
        }"#,
    )
    .unwrap();
    let prepared: serde_json::Value = serde_json::from_str(&prepared).unwrap();
    assert_eq!(prepared["setup_state"], "pending_callback");
    assert!(prepared.get("callback_verified_at").is_none());
    assert_eq!(prepared["corp_id"], "ww-test");
}
