//! Typed config patching for chat channels.
//!
//! The channel's `config_json` is owned by the backend: internal fields such
//! as `channel_workspace_root` must never be overwritten by the UI, and
//! unknown fields must round-trip untouched. UI updates therefore arrive as a
//! field-level patch that is merged into the *current* stored JSON — never as
//! a rebuilt full object.

use serde::Deserialize;

/// Fields the frontend is allowed to set. `Option<Option<T>>`:
/// - `None`        → field not present in the patch, leave untouched
/// - `Some(None)`  → explicit null, delete the field
/// - `Some(Some(v))` → set the field to `v`
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatChannelConfigPatch {
    pub base_url: Option<Option<String>>,
    pub app_id: Option<Option<String>>,
    pub chat_id: Option<Option<String>>,
    pub default_chatid: Option<Option<String>>,
    pub default_chat_type: Option<Option<u8>>,
    pub default_agent_type: Option<Option<String>>,
    pub poll_interval_secs: Option<Option<u64>>,
    /// Explicit deletion of additional (unknown) keys. Must not target
    /// protected/internal fields.
    #[serde(default)]
    pub delete_fields: Vec<String>,
}

/// Internal fields the UI must never touch.
pub const PROTECTED_CONFIG_FIELDS: &[&str] = &["channel_workspace_root", "default_folder_id"];

/// Apply a typed patch onto the stored config JSON.
///
/// Returns an error (never a silent `{}` fallback) when the stored JSON is
/// missing, unparseable, or not an object, or when the patch tries to delete
/// a protected field.
pub fn apply_config_patch(
    current_json: &str,
    patch: &ChatChannelConfigPatch,
) -> Result<String, String> {
    let mut map = parse_config_object(current_json)?;

    for field in &patch.delete_fields {
        let field = field.trim();
        if field.is_empty() {
            continue;
        }
        if PROTECTED_CONFIG_FIELDS.contains(&field) {
            return Err(format!(
                "field `{field}` is managed by the backend and cannot be deleted"
            ));
        }
        map.remove(field);
    }

    set_or_clear(&mut map, "base_url", patch.base_url.as_ref());
    set_or_clear(&mut map, "app_id", patch.app_id.as_ref());
    set_or_clear(&mut map, "chat_id", patch.chat_id.as_ref());
    set_or_clear(&mut map, "default_chatid", patch.default_chatid.as_ref());
    set_or_clear(&mut map, "default_chat_type", patch.default_chat_type.as_ref());
    set_or_clear(&mut map, "default_agent_type", patch.default_agent_type.as_ref());
    set_or_clear(&mut map, "poll_interval_secs", patch.poll_interval_secs.as_ref());

    serde_json::to_string(&serde_json::Value::Object(map))
        .map_err(|e| format!("failed to serialize patched config: {e}"))
}

/// Parse stored config JSON into an object map, failing loudly instead of
/// overwriting corrupt state with `{}` (IYW-CHANNEL-004 / -005).
pub fn parse_config_object(current_json: &str) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let value: serde_json::Value = serde_json::from_str(current_json)
        .map_err(|e| format!("stored config is not valid JSON: {e}"))?;
    match value {
        serde_json::Value::Object(map) => Ok(map),
        _ => Err("stored config must be a JSON object".to_string()),
    }
}

fn set_or_clear(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    patch: Option<&Option<serde_json::Value>>,
) {
    match patch {
        Some(Some(value)) => {
            map.insert(key.to_string(), value.clone());
        }
        Some(None) => {
            map.remove(key);
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(json: &str) -> ChatChannelConfigPatch {
        serde_json::from_str(json).expect("patch should parse")
    }

    #[test]
    fn preserves_internal_and_unknown_fields() {
        let current = r#"{
            "base_url": "https://example.com",
            "channel_workspace_root": "C:/ws/1",
            "future_field": {"nested": true},
            "default_agent_type": "codex"
        }"#;
        let p = patch(r#"{"baseUrl": "https://new.example.com"}"#);
        let out: serde_json::Value =
            serde_json::from_str(&apply_config_patch(current, &p).unwrap()).unwrap();
        assert_eq!(out["base_url"], "https://new.example.com");
        // Internal + unknown fields survive untouched.
        assert_eq!(out["channel_workspace_root"], "C:/ws/1");
        assert_eq!(out["future_field"]["nested"], true);
        assert_eq!(out["default_agent_type"], "codex");
    }

    #[test]
    fn explicit_null_deletes_field() {
        let current = r#"{"default_agent_type": "codex", "chat_id": "oc_x"}"#;
        let p = patch(r#"{"defaultAgentType": null}"#);
        let out: serde_json::Value =
            serde_json::from_str(&apply_config_patch(current, &p).unwrap()).unwrap();
        assert!(out.get("default_agent_type").is_none());
        assert_eq!(out["chat_id"], "oc_x");
    }

    #[test]
    fn delete_fields_removes_unknown_keys() {
        let current = r#"{"legacy_opt": "x", "channel_workspace_root": "C:/ws/1"}"#;
        let p = patch(r#"{"deleteFields": ["legacy_opt"]}"#);
        let out: serde_json::Value =
            serde_json::from_str(&apply_config_patch(current, &p).unwrap()).unwrap();
        assert!(out.get("legacy_opt").is_none());
        assert_eq!(out["channel_workspace_root"], "C:/ws/1");
    }

    #[test]
    fn protected_field_delete_is_rejected() {
        let current = r#"{"channel_workspace_root": "C:/ws/1"}"#;
        let p = patch(r#"{"deleteFields": ["channel_workspace_root"]}"#);
        assert!(apply_config_patch(current, &p).is_err());
    }

    #[test]
    fn corrupt_config_is_not_silently_replaced() {
        let p = patch(r#"{"baseUrl": "https://x"}"#);
        assert!(apply_config_patch("not-json{{{", &p).is_err());
        assert!(apply_config_patch("[]", &p).is_err());
    }

    #[test]
    fn wechat_qr_patch_only_touches_base_url() {
        let current = r#"{
            "base_url": "https://old.example.com",
            "channel_workspace_root": "C:/ws/1",
            "default_agent_type": "claude_code"
        }"#;
        // Mirrors `weixin_check_qrcode` confirmation: patch base_url only.
        let p = patch(r#"{"baseUrl": "https://ilinkai.weixin.qq.com"}"#);
        let out: serde_json::Value =
            serde_json::from_str(&apply_config_patch(current, &p).unwrap()).unwrap();
        assert_eq!(out["base_url"], "https://ilinkai.weixin.qq.com");
        assert_eq!(out["channel_workspace_root"], "C:/ws/1");
        assert_eq!(out["default_agent_type"], "claude_code");
    }
}
