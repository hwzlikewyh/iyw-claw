use super::*;
use crate::acp::provider_overlay::{MANAGED_DEFAULT_MODEL, MANAGED_MODEL_IDS};
use crate::acp::types::SessionConfigKindInfo;

fn session_meta() -> serde_json::Map<String, serde_json::Value> {
    serde_json::from_value(serde_json::json!({
        "x.ai/sessionConfig": {
            "options": [
                {"id": "grok-4.5", "category": "model", "label": "Grok 4.5", "selected": true},
                {"id": "deepseek-v4-pro", "category": "model", "label": "DeepSeek V4 Pro"},
                {"id": "doubao-seed-2-1-pro-260628", "category": "model", "label": "Doubao Seed"},
                {"id": "deepseek-v4-flash", "category": "model", "label": "DeepSeek V4 Flash"},
                {"id": "low", "category": "mode", "label": "Low"},
                {"id": "high", "category": "mode", "label": "High", "selected": true}
            ]
        }
    }))
    .expect("session metadata")
}

fn model_metadata() -> serde_json::Value {
    serde_json::json!({
        "availableModels": [{
            "modelId": "deepseek-v4-pro",
            "_meta": {
                "supportsReasoningEffort": true,
                "reasoningEffort": "xhigh",
                "reasoningEfforts": [
                    {"id": "high", "label": "Highest", "description": "Highest quality"},
                    {"id": "low", "label": "Fast", "description": "Fastest"}
                ]
            }
        }]
    })
}

#[test]
fn selectors_only_expose_codex_managed_models() {
    let options =
        synthesize_options(Some(&session_meta()), &EffortSpecs::new()).expect("managed selectors");
    let model = options.iter().find(|option| option.id == "model").unwrap();
    let SessionConfigKindInfo::Select(select) = &model.kind;

    assert_eq!(select.current_value, MANAGED_DEFAULT_MODEL);
    assert_eq!(
        select
            .options
            .iter()
            .map(|option| option.value.as_str())
            .collect::<Vec<_>>(),
        MANAGED_MODEL_IDS
    );
    assert!(select
        .options
        .iter()
        .all(|option| option.value != "grok-4.5"));
}

#[test]
fn reasoning_selector_tracks_the_current_managed_model() {
    let specs = parse_effort_specs(Some(&model_metadata()));
    let options = synthesize_options(Some(&session_meta()), &specs).expect("selectors");
    let effort = options
        .iter()
        .find(|option| option.id == "reasoning_effort")
        .expect("reasoning selector");
    let SessionConfigKindInfo::Select(select) = &effort.kind;

    assert_eq!(select.current_value, "xhigh");
    assert_eq!(select.options[0].value, "xhigh");
    assert_eq!(select.options[0].name, "Max");
}

#[test]
fn set_model_params_carry_optional_reasoning_effort() {
    let plain = build_set_model_params("session-1", "deepseek-v4-pro", None);
    assert_eq!(plain["sessionId"], "session-1");
    assert_eq!(plain["modelId"], "deepseek-v4-pro");
    assert!(plain.get("_meta").is_none());

    let reasoned = build_set_model_params("session-1", "deepseek-v4-pro", Some("high"));
    assert_eq!(reasoned["_meta"]["reasoningEffort"], "high");
}
