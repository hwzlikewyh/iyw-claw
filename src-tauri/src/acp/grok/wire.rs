use std::collections::BTreeMap;
use std::sync::Arc;

use sacp::schema::SessionId;
use sacp::{Agent, ConnectionTo, UntypedMessage};
use tokio::sync::RwLock;

use super::{build_set_model_params, set_effort_selector_for_model, EffortSpecs};
use crate::acp::session_state::SessionState;
use crate::acp::types::{AcpEvent, SessionConfigKindInfo, SessionConfigOptionInfo};
use crate::models::agent::AgentType;
use crate::web::event_bridge::{emit_with_state, EventEmitter};

const INCOMPATIBLE_AGENT_ERROR_CODE: &str = "grok_model_switch_incompatible_agent";

fn current_model_from_options(options: &[SessionConfigOptionInfo]) -> Option<String> {
    options
        .iter()
        .find(|option| option.id == "model")
        .map(|option| {
            let SessionConfigKindInfo::Select(select) = &option.kind;
            select.current_value.clone()
        })
}

async fn current_model(state: &Arc<RwLock<SessionState>>) -> Option<String> {
    let options = state.read().await.config_options.clone()?;
    current_model_from_options(&options)
}

fn resolve_set_model_target(
    config_id: &str,
    value_id: &str,
    current_model: Option<&str>,
) -> Option<(String, Option<String>)> {
    match config_id {
        "model" if !value_id.trim().is_empty() => Some((value_id.to_string(), None)),
        "reasoning_effort" => {
            let model = current_model.filter(|model| !model.trim().is_empty())?;
            Some((model.to_string(), Some(value_id.to_string())))
        }
        _ => None,
    }
}

async fn send_set_model(
    cx: &ConnectionTo<Agent>,
    session_id: &SessionId,
    model_id: &str,
    effort: Option<&str>,
) -> Result<(), sacp::Error> {
    let params = build_set_model_params(session_id.0.as_ref(), model_id, effort);
    let request = UntypedMessage::new("session/set_model", params).map_err(|error| {
        sacp::util::internal_error(format!("Failed to build set_model request: {error}"))
    })?;
    cx.send_request_to(Agent, request).block_task().await?;
    Ok(())
}

fn is_incompatible_agent_switch(error: &sacp::Error) -> bool {
    error
        .data
        .as_ref()
        .and_then(|data| data.get("code"))
        .and_then(serde_json::Value::as_str)
        == Some("MODEL_SWITCH_INCOMPATIBLE_AGENT")
}

async fn emit_options(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    options: Vec<SessionConfigOptionInfo>,
) {
    emit_with_state(
        state,
        emitter,
        AcpEvent::SessionConfigOptions {
            config_options: options,
        },
    )
    .await;
}

async fn emit_incompatible_switch(state: &Arc<RwLock<SessionState>>, emitter: &EventEmitter) {
    let current = state.read().await.config_options.clone();
    if let Some(options) = current {
        emit_options(state, emitter, options).await;
    }
    emit_with_state(
        state,
        emitter,
        AcpEvent::Error {
            message: "This conversation cannot switch to that managed model. Start a new session to use it."
                .to_string(),
            agent_type: AgentType::Grok.to_string(),
            code: Some(INCOMPATIBLE_AGENT_ERROR_CODE.to_string()),
            terminal: false,
        },
    )
    .await;
}

fn update_options(
    options: &mut Vec<SessionConfigOptionInfo>,
    config_id: &str,
    value_id: &str,
    specs: &EffortSpecs,
) {
    if let Some(option) = options.iter_mut().find(|option| option.id == config_id) {
        let SessionConfigKindInfo::Select(select) = &mut option.kind;
        select.current_value = value_id.to_string();
    }
    if config_id == "model" {
        set_effort_selector_for_model(options, value_id, specs);
    }
}

async fn update_state_and_emit(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    config_id: &str,
    value_id: &str,
) {
    let (mut options, specs) = {
        let session = state.read().await;
        (
            session.config_options.clone().unwrap_or_default(),
            session.grok_effort_specs.clone().unwrap_or_default(),
        )
    };
    update_options(&mut options, config_id, value_id, &specs);
    emit_options(state, emitter, options).await;
}

pub(crate) async fn set_config_option(
    cx: &ConnectionTo<Agent>,
    session_id: &SessionId,
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    config_id: String,
    value_id: String,
) -> Result<(), sacp::Error> {
    let current = current_model(state).await;
    let Some((model_id, effort)) =
        resolve_set_model_target(&config_id, &value_id, current.as_deref())
    else {
        return Ok(());
    };
    match send_set_model(cx, session_id, &model_id, effort.as_deref()).await {
        Ok(()) => update_state_and_emit(state, emitter, &config_id, &value_id).await,
        Err(error) if is_incompatible_agent_switch(&error) => {
            emit_incompatible_switch(state, emitter).await;
        }
        Err(error) => return Err(error),
    }
    Ok(())
}

fn offered_value(options: &[SessionConfigOptionInfo], config_id: &str, value_id: &str) -> bool {
    options
        .iter()
        .find(|option| option.id == config_id)
        .is_some_and(|option| {
            let SessionConfigKindInfo::Select(select) = &option.kind;
            select.current_value != value_id
                && select.options.iter().any(|option| option.value == value_id)
        })
}

async fn apply_preference(
    cx: &ConnectionTo<Agent>,
    session_id: &SessionId,
    options: &mut Vec<SessionConfigOptionInfo>,
    specs: &EffortSpecs,
    config_id: &str,
    value_id: &str,
) {
    if !offered_value(options, config_id, value_id) {
        return;
    }
    let current = current_model_from_options(options);
    let Some((model_id, effort)) =
        resolve_set_model_target(config_id, value_id, current.as_deref())
    else {
        return;
    };
    match send_set_model(cx, session_id, &model_id, effort.as_deref()).await {
        Ok(()) => update_options(options, config_id, value_id, specs),
        Err(error) => {
            tracing::warn!("[ACP] failed to apply preferred Grok {config_id}='{value_id}': {error}")
        }
    }
}

pub(crate) async fn apply_preferred_options(
    cx: &ConnectionTo<Agent>,
    session_id: &SessionId,
    options: &mut Vec<SessionConfigOptionInfo>,
    preferences: &BTreeMap<String, String>,
    specs: &EffortSpecs,
) {
    for config_id in ["model", "reasoning_effort"] {
        if let Some(value_id) = preferences.get(config_id) {
            apply_preference(cx, session_id, options, specs, config_id, value_id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::RwLock;

    use super::*;
    use crate::acp::grok::{synthesize_options, EffortSpecs};
    use crate::acp::session_state::SessionState;
    use crate::acp::types::AcpEvent;
    use crate::models::agent::AgentType;
    use crate::web::event_bridge::EventEmitter;

    #[test]
    fn model_target_accepts_online_catalog_models() {
        assert_eq!(
            resolve_set_model_target("model", "online-only", None),
            Some(("online-only".to_string(), None))
        );
        assert_eq!(resolve_set_model_target("model", "", None), None);
        assert_eq!(
            resolve_set_model_target("reasoning_effort", "high", Some("online-only"),),
            Some(("online-only".to_string(), Some("high".to_string())))
        );
    }

    #[tokio::test]
    async fn incompatible_switch_reemits_authoritative_options_before_error() {
        let mut session = SessionState::new(
            "conn-grok".to_string(),
            AgentType::Grok,
            None,
            "main".to_string(),
            None,
        );
        // Options come from agent metadata since the online-catalog cutover
        // (no local fallback list), so seed the synthesizer with one model.
        let meta = serde_json::json!({
            "x.ai/sessionConfig": {
                "options": [{ "id": "grok-4", "label": "Grok 4", "selected": true }]
            }
        });
        session.config_options = synthesize_options(meta.as_object(), &EffortSpecs::new());
        assert!(
            session.config_options.is_some(),
            "seed metadata must synthesize a non-empty option list"
        );
        let state = Arc::new(RwLock::new(session));
        let emitter = EventEmitter::Noop;

        tokio::time::timeout(
            Duration::from_secs(2),
            emit_incompatible_switch(&state, &emitter),
        )
        .await
        .expect("rollback must not deadlock");

        let events = state.read().await.recent_events_after(0).expect("events");
        assert!(matches!(
            events.first().map(|event| &event.payload),
            Some(AcpEvent::SessionConfigOptions { .. })
        ));
        assert!(matches!(
            events.get(1).map(|event| &event.payload),
            Some(AcpEvent::Error {
                code: Some(code),
                terminal: false,
                ..
            }) if code == INCOMPATIBLE_AGENT_ERROR_CODE
        ));
    }
}
