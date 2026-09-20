use sacp::schema::{SessionConfigKind, SessionConfigOption, SessionId};
use sacp::{Agent, ConnectionTo, UntypedMessage};
use serde_json::{json, Value};

use super::types::{SessionConfigKindInfo, SessionConfigOptionInfo};
use crate::models::AgentType;

pub(super) fn attach_options(response: &mut Value, agent: AgentType) {
    if agent != AgentType::Hermes {
        return;
    }
    let Some(models) = response.get("models") else {
        return;
    };
    let Some(current) = models.get("currentModelId").and_then(Value::as_str) else {
        return;
    };
    let Some(available) = models.get("availableModels").and_then(Value::as_array) else {
        return;
    };
    let choices = available
        .iter()
        .filter_map(|model| {
            let id = model.get("modelId")?.as_str()?;
            Some(json!({
                "value": id,
                "name": model.get("name").and_then(Value::as_str).unwrap_or(id),
                "description": model.get("description").and_then(Value::as_str),
            }))
        })
        .collect::<Vec<_>>();
    let model = json!({
        "id": "model", "name": "Model", "category": "model", "type": "select",
        "currentValue": current, "options": choices,
    });
    let mut options = response
        .get("configOptions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !options
        .iter()
        .any(|option| option.get("id").and_then(Value::as_str) == Some("model"))
    {
        options.push(model);
    }
    response["configOptions"] = Value::Array(options);
}

pub(super) async fn set_model(
    cx: &ConnectionTo<Agent>,
    session_id: &SessionId,
    model: &str,
) -> Result<(), sacp::Error> {
    let request = UntypedMessage::new(
        "session/set_model",
        json!({
            "sessionId": session_id, "modelId": model,
        }),
    )
    .map_err(|error| sacp::util::internal_error(error.to_string()))?;
    cx.send_request_to(Agent, request).block_task().await?;
    Ok(())
}

pub(super) fn select_model(
    options: &mut [SessionConfigOptionInfo],
    model: &str,
) -> Result<(), sacp::Error> {
    let option = options
        .iter_mut()
        .find(|option| option.id == "model")
        .ok_or_else(|| sacp::util::internal_error("Agent did not advertise a model selector"))?;
    let SessionConfigKindInfo::Select(select) = &mut option.kind;
    if !select.options.iter().any(|option| option.value == model) {
        return Err(sacp::util::internal_error(
            "Selected model is not advertised by the Agent",
        ));
    }
    select.current_value = model.to_string();
    Ok(())
}

pub(super) async fn apply_preference(
    cx: &ConnectionTo<Agent>,
    session_id: &SessionId,
    selection: (&mut [SessionConfigOption], &str),
) -> Result<(), sacp::Error> {
    let (options, model) = selection;
    set_model(cx, session_id, model).await?;
    let Some(option) = options
        .iter_mut()
        .find(|option| option.id.to_string() == "model")
    else {
        return Err(sacp::util::internal_error(
            "Agent model selector disappeared",
        ));
    };
    if let SessionConfigKind::Select(select) = &mut option.kind {
        select.current_value = model.to_string().into();
    }
    Ok(())
}
