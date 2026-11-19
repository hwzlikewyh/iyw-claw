use super::{
    connection::ConnectionCommand, error::AcpError, manager::ConnectionManager,
    session_state::SessionState,
};
use sacp::{Agent, ConnectionTo, UntypedMessage};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{oneshot, RwLock};

pub(super) type Reply = oneshot::Sender<Result<bool, AcpError>>;
type Receipt = oneshot::Receiver<Result<bool, AcpError>>;

impl ConnectionManager {
    pub(super) async fn input_boundary(
        &self,
        id: &str,
        generation: i64,
        interrupt: bool,
    ) -> Result<Receipt, AcpError> {
        let tx = self
            .connections
            .lock()
            .await
            .get(id)
            .ok_or_else(|| AcpError::ConnectionNotFound(id.to_owned()))?
            .cmd_tx
            .clone();
        let (reply, receipt) = oneshot::channel();
        tx.send(ConnectionCommand::InputBoundary {
            generation,
            interrupt,
            reply,
        })
        .await
        .map_err(|_| AcpError::ProcessExited)?;
        Ok(receipt)
    }
}

pub(super) async fn dispatch(
    connection: ConnectionTo<Agent>,
    session: &str,
    state: &Arc<RwLock<SessionState>>,
    generation: i64,
    interrupt: bool,
    mut reply: Reply,
) {
    let current = state.read().await;
    let interrupt = interrupt && current.turn_in_flight && current.turn_generation == generation;
    drop(current);
    let params = json!({ "sessionId": session, "interrupt": interrupt });
    tokio::spawn(async move {
        let result = tokio::select! {
            _ = reply.closed() => return,
            result = request(&connection, params) => result,
        };
        let _ = reply.send(result);
    });
}

async fn request(connection: &ConnectionTo<Agent>, params: Value) -> Result<bool, AcpError> {
    let message = UntypedMessage::new("_iyw/input_boundary", params)
        .map_err(|e| AcpError::protocol(e.to_string()))?;
    let response: Result<Value, _> = connection
        .send_request_to(Agent, message)
        .block_task()
        .await;
    match response {
        Ok(value) if value["ready"] == true => Ok(true),
        Ok(value) if value["supported"] == false => Ok(false),
        Err(error) if i32::from(error.code) == -32601 => Ok(false),
        Err(error) => Err(AcpError::protocol(format!(
            "input boundary RPC failed ({})",
            i32::from(error.code)
        ))),
        Ok(_) => Err(AcpError::protocol("invalid input boundary receipt")),
    }
}

#[derive(Default)]
pub(super) struct NativeBoundary {
    state: Option<(i64, bool, Receipt)>,
    ready: Option<(i64, bool)>,
    unsupported: bool,
}

pub(super) enum BoundaryResult {
    Ready,
    Changed,
    Unsupported,
    Failed(AcpError),
}

impl NativeBoundary {
    pub(super) fn reset_receipt(&mut self) {
        self.ready = None;
        self.state = None;
    }

    pub(super) async fn wait(
        &mut self,
        context: &super::agent_input_worker::WorkerContext<'_>,
        snapshot: &mut super::agent_input_worker::WorkerSnapshot,
        interrupt: bool,
    ) -> BoundaryResult {
        if self.unsupported || !snapshot.native_input_boundary {
            return BoundaryResult::Unsupported;
        }
        let generation = snapshot.turn_generation;
        if self.ready == Some((generation, interrupt)) {
            return BoundaryResult::Ready;
        }
        if !self
            .state
            .as_ref()
            .is_some_and(|(g, i, _)| (*g, *i) == (generation, interrupt))
        {
            match context
                .manager
                .input_boundary(context.conn_id, generation, interrupt)
                .await
            {
                Ok(receipt) => self.state = Some((generation, interrupt, receipt)),
                Err(e) => return BoundaryResult::Failed(e),
            }
        }
        let (_, _, receipt) = self.state.as_mut().expect("input receipt registered");
        let result = tokio::select! {
            result = receipt => result.unwrap_or(Err(AcpError::ProcessExited)),
            _ = snapshot.wake.as_mut() => return BoundaryResult::Changed,
        };
        self.state = None;
        match result {
            Ok(true) => {
                self.ready = Some((generation, interrupt));
                BoundaryResult::Ready
            }
            Ok(false) => {
                self.unsupported = true;
                BoundaryResult::Unsupported
            }
            Err(e) => BoundaryResult::Failed(e),
        }
    }
}
