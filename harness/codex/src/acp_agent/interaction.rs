use sacp::{Client, ConnectionTo, UntypedMessage};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::AdmittedServerRequest;

use super::interaction_registry::{InteractionLease, InteractionRegistry, RegisteredInteraction};
use super::{interaction_mapping::InteractionPlan, to_sacp_error, BridgeCommand};

pub(super) struct InteractionRequest {
    pub admission: AdmittedServerRequest,
    pub params: Value,
    pub form_supported: bool,
    pub request_id: String,
    pub registry: InteractionRegistry,
}

pub(super) fn is_method(method: &str) -> bool {
    matches!(
        method,
        "item/tool/requestUserInput"
            | "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "mcpServer/elicitation/request"
            | "item/permissions/requestApproval"
            | "currentTime/read"
    )
}

pub(super) fn forward(
    cx: &ConnectionTo<Client>,
    command_tx: &mpsc::Sender<BridgeCommand>,
    request: InteractionRequest,
) -> Result<(), sacp::Error> {
    let plan = InteractionPlan::new(
        &request.admission.method,
        &request.params,
        request.form_supported,
    )
    .map_err(to_sacp_error)?;
    let mut lease = None;
    let form_gate = plan
        .wire_request()
        .filter(|(method, _)| *method == "elicitation/create")
        .map(|_| request.registry.form_gate());
    let wire = plan
        .wire_request()
        .map(|(method, params)| {
            let (message, guard) = prepare_wire(&request, (method, params.clone()))?;
            lease = guard;
            Ok::<_, sacp::Error>(message)
        })
        .transpose()?;
    let command_tx = command_tx.clone();
    let connection = cx.clone();
    cx.spawn(async move {
        // 子代理共享父会话的一张问答卡，排队后再发送，避免宿主拒绝并发提问。
        let _form_guard = match form_gate {
            Some(gate) => Some(gate.lock_owned().await),
            None => None,
        };
        let _lease = lease;
        let response = match wire {
            Some(_) if _lease.as_ref().is_some_and(|lease| !lease.is_active()) => {
                Err("interaction was resolved before presentation".to_string())
            }
            Some(message) => match connection
                .send_request_to(Client, message)
                .block_task()
                .await
            {
                Ok(value) => plan.response(value).map_err(|error| error.to_string()),
                Err(error) => Err(error.to_string()),
            },
            None => plan
                .response(Value::Null)
                .map_err(|error| error.to_string()),
        };
        let _ = command_tx
            .send(BridgeCommand::ServerResponse {
                token: request.admission.token,
                target: request.admission.target,
                method: request.admission.method,
                turn_id: request.admission.turn_id,
                response,
            })
            .await;
        Ok(())
    })
}

fn prepare_wire(
    request: &InteractionRequest,
    wire: (&str, Value),
) -> Result<(UntypedMessage, Option<InteractionLease>), sacp::Error> {
    let (method, mut params) = wire;
    let mut lease = None;
    if matches!(method, "elicitation/create" | "session/request_permission") {
        let key = format!("{method}:{}", request.admission.token.value());
        let session = params["sessionId"].as_str().unwrap_or_default().to_string();
        if !params["_meta"].is_object() {
            params["_meta"] = serde_json::json!({});
        }
        params["_meta"]["iyw"] = serde_json::json!({ "requestKey": key });
        lease = Some(request.registry.register(
            request.request_id.clone(),
            RegisteredInteraction {
                session,
                key,
                thread: match &request.admission.target {
                    crate::ServerRequestTarget::Session(binding) => {
                        Some(binding.external_id.clone())
                    }
                    _ => None,
                },
                turn: request.admission.turn_id.clone(),
            },
        ));
    }
    Ok((UntypedMessage::new(method, params)?, lease))
}
