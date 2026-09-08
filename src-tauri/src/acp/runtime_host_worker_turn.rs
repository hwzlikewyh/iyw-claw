use std::time::Duration;

use sacp::schema::SessionId;
use sacp::{Agent, ConnectionTo, JsonRpcRequest, Responder};

use crate::acp::runtime_host_router::SessionRequestRouter;

const ADOPTION_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonRpcRequest)]
#[request(method = "_iyw/worker/turn_started", response = serde_json::Value)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkerTurnStartedRequest {
    session_id: SessionId,
    turn_id: String,
    after_generation: Option<i64>,
}

impl SessionRequestRouter {
    pub(super) async fn worker_turn_started(
        &self,
        request: WorkerTurnStartedRequest,
        responder: Responder<serde_json::Value>,
        connection: ConnectionTo<Agent>,
    ) -> Result<(), sacp::Error> {
        let router = self.clone();
        connection.spawn(async move {
            match router.adopt_worker_turn(&request).await {
                Ok(value) => responder.respond(value),
                Err(error) => {
                    if let Some(route) = router.resolve(&request.session_id) {
                        crate::acp::agent_input_native_turn::cancel_pending_automatic(
                            &route.state,
                            &request.turn_id,
                        )
                        .await;
                    }
                    tracing::warn!(session_id = %request.session_id, turn_id = %request.turn_id,
                        error = %error, "[星河][worker] automatic turn adoption failed");
                    responder.respond_with_error(sacp::util::internal_error(error))
                }
            }
        })
    }

    async fn adopt_worker_turn(
        &self,
        request: &WorkerTurnStartedRequest,
    ) -> Result<serde_json::Value, String> {
        let route = self
            .resolve(&request.session_id)
            .ok_or("worker session route expired")?;
        let db = route
            .worker_database
            .as_ref()
            .ok_or("route is not an internal worker")?;
        if request.turn_id.trim().is_empty() {
            return Err("worker turn id is empty".into());
        }
        let result = tokio::time::timeout(ADOPTION_TIMEOUT, async {
            if !register_automatic(&route, request, db).await? {
                return Ok(false);
            }
            wait_for_adoption(&route.state, &request.turn_id).await
        })
        .await;
        let accepted = match result {
            Ok(result) => result,
            Err(_)
                if crate::acp::agent_input_native_turn::cancel_pending_automatic(
                    &route.state,
                    &request.turn_id,
                )
                .await =>
            {
                Ok(true)
            }
            Err(_) => Err("automatic turn adoption timed out".into()),
        }?;
        Ok(
            serde_json::json!({ "accepted": accepted, "queuedPrompt": !accepted, "generation": route.state.read().await.turn_generation }),
        )
    }
}

async fn wait_for_adoption(
    state: &std::sync::Arc<tokio::sync::RwLock<crate::acp::session_state::SessionState>>,
    turn_id: &str,
) -> Result<bool, String> {
    loop {
        let wake = {
            let snapshot = state.read().await;
            let turn = snapshot
                .native_background_turn
                .as_ref()
                .ok_or("automatic turn was cancelled")?;
            if turn.message_id != turn_id {
                return Err("automatic turn was replaced".into());
            }
            if turn.adopted_generation.is_some() {
                return Ok(true);
            }
            snapshot.native_background_notify.clone().notified_owned()
        };
        wake.await;
    }
}

async fn register_automatic(
    route: &crate::acp::runtime_host_router::RuntimeSessionRoute,
    request: &WorkerTurnStartedRequest,
    db: &sea_orm::DatabaseConnection,
) -> Result<bool, String> {
    loop {
        let wake = route
            .state
            .read()
            .await
            .native_background_notify
            .clone()
            .notified_owned();
        if let Some(registered) = crate::acp::agent_input_native_turn::begin_automatic(
            &route.state,
            &route.emitter,
            (db, &request.turn_id, request.after_generation),
        )
        .await?
        {
            return Ok(registered);
        }
        wake.await;
    }
}

pub(crate) async fn prompt_request(
    state: &std::sync::Arc<tokio::sync::RwLock<crate::acp::session_state::SessionState>>,
    mut request: sacp::schema::PromptRequest,
) -> sacp::schema::PromptRequest {
    let snapshot = state.read().await;
    if crate::internal_xinghe_worker::is_desktop_agent(snapshot.agent_type) {
        request.meta.get_or_insert_with(Default::default).insert(
            "iyw".into(),
            serde_json::json!({ "turnGeneration": snapshot.turn_generation }),
        );
    }
    request
}
