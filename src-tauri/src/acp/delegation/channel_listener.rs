use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::listener::{DelegationListener, TokenEntry};
use super::transport::{write_frame, BrokerChannelRequest, BrokerResponse};
use crate::acp::channel_tools::{ChannelCaller, ChannelToolService, CHANNEL_TOOL_NAMES};

impl DelegationListener {
    pub(super) async fn serve_channel<C>(
        &self,
        conn: &mut C,
        request: BrokerChannelRequest,
    ) -> std::io::Result<()>
    where
        C: AsyncReadExt + AsyncWriteExt + Unpin + Send,
    {
        let Some(entry) = self.tokens.lookup(&request.token).await else {
            return write_outcome(conn, serde_json::json!({ "error": "INVALID_TOKEN" })).await;
        };
        if !CHANNEL_TOOL_NAMES.contains(&request.tool.as_str()) {
            return write_outcome(conn, serde_json::json!({ "error": "UNKNOWN_CHANNEL_TOOL" }))
                .await;
        }
        let caller = channel_caller(&entry);
        if ChannelToolService::requires_confirmation(&request.tool, &request.input) {
            return self
                .serve_channel_confirmation(conn, request, entry, caller)
                .await;
        }
        self.execute_channel_request(conn, &entry, caller, request, None)
            .await
    }

    async fn serve_channel_confirmation<C>(
        &self,
        conn: &mut C,
        request: BrokerChannelRequest,
        entry: TokenEntry,
        caller: ChannelCaller,
    ) -> std::io::Result<()>
    where
        C: AsyncReadExt + AsyncWriteExt + Unpin + Send,
    {
        let spec = match self
            .channel_tools
            .prepare_confirmation(&request.tool, &request.input)
            .await
        {
            Ok(spec) => spec,
            Err(error) => return write_outcome(conn, serde_json::json!({ "error": error })).await,
        };
        let Some(registered) = self.register_confirmation(&entry, &spec.state).await else {
            if entry.cancellation.is_cancelled() {
                return Ok(());
            }
            return write_outcome(conn, serde_json::json!({ "error": "CONFIRMATION_BUSY" })).await;
        };
        let outcome = self
            .await_channel_confirmation(conn, &entry, registered)
            .await?;
        let Some(confirmed) = outcome else {
            return Ok(());
        };
        if let Some(error) = confirmation_error(
            confirmed,
            self.tokens.lookup(&request.token).await.is_some(),
        ) {
            return write_outcome(conn, serde_json::json!({ "error": error })).await;
        }
        self.execute_channel_request(conn, &entry, caller, request, Some(spec.resource_version))
            .await
    }

    async fn register_confirmation(
        &self,
        entry: &TokenEntry,
        state: &crate::acp::channel_tools::confirmation::PendingChannelConfirmationState,
    ) -> Option<crate::acp::channel_tools::confirmation::RegisteredChannelConfirmation> {
        let registered = self
            .confirmations
            .register_channel_confirmation(&entry.parent_connection_id, state.clone())
            .await?;
        if entry.cancellation.is_cancelled() {
            self.confirmations
                .cancel_channel_confirmation(
                    &entry.parent_connection_id,
                    &registered.confirmation_id,
                )
                .await;
            return None;
        }
        Some(registered)
    }

    async fn execute_channel_request<C>(
        &self,
        conn: &mut C,
        entry: &TokenEntry,
        caller: ChannelCaller,
        request: BrokerChannelRequest,
        expected_version: Option<String>,
    ) -> std::io::Result<()>
    where
        C: AsyncReadExt + AsyncWriteExt + Unpin + Send,
    {
        let input_for_cancel = request.input.clone();
        let caller_for_cancel = caller.clone();
        let execute = async {
            match expected_version {
                Some(version) => {
                    self.channel_tools
                        .execute_confirmed(caller, &request.tool, request.input, &version)
                        .await
                }
                None => {
                    self.channel_tools
                        .execute(caller, &request.tool, request.input)
                        .await
                }
            }
        };
        tokio::pin!(execute);
        let mut probe = [0u8; 1];
        let outcome = tokio::select! {
            outcome = &mut execute => Some(outcome),
            _ = entry.cancellation.cancelled() => None,
            _ = conn.read(&mut probe) => None,
        };
        drop(execute);
        let Some(outcome) = outcome else {
            self.channel_tools
                .cancel_request(&caller_for_cancel, &request.tool, &input_for_cancel)
                .await;
            return Ok(());
        };
        write_outcome(conn, outcome).await
    }

    async fn await_channel_confirmation<C>(
        &self,
        conn: &mut C,
        entry: &TokenEntry,
        registered: crate::acp::channel_tools::confirmation::RegisteredChannelConfirmation,
    ) -> std::io::Result<Option<Option<bool>>>
    where
        C: AsyncReadExt + AsyncWriteExt + Unpin + Send,
    {
        let confirmation_id = registered.confirmation_id;
        let mut outcome_rx = registered.outcome_rx;
        let wait = tokio::time::timeout(std::time::Duration::from_secs(300), &mut outcome_rx);
        tokio::pin!(wait);
        let mut probe = [0u8; 1];
        tokio::select! {
            outcome = &mut wait => {
                let resolved = outcome.ok().and_then(Result::ok).map(|value| value.confirmed);
                if resolved.is_none() {
                    self.cancel_confirmation(entry, &confirmation_id).await;
                }
                Ok(Some(resolved))
            },
            _ = conn.read(&mut probe) => {
                self.cancel_confirmation(entry, &confirmation_id).await;
                Ok(None)
            },
            _ = entry.cancellation.cancelled() => {
                self.cancel_confirmation(entry, &confirmation_id).await;
                Ok(None)
            }
        }
    }

    async fn cancel_confirmation(&self, entry: &TokenEntry, confirmation_id: &str) {
        self.confirmations
            .cancel_channel_confirmation(&entry.parent_connection_id, confirmation_id)
            .await;
    }
}

fn channel_caller(entry: &TokenEntry) -> ChannelCaller {
    ChannelCaller {
        agent_type: entry.agent_type.to_string(),
        session_ref: entry.opaque_source_id.clone(),
        caller_scope: format!("{}:{}", entry.agent_type, entry.opaque_source_id),
        working_dir: entry.working_dir.clone(),
    }
}

fn confirmation_error(confirmed: Option<bool>, token_valid: bool) -> Option<&'static str> {
    match (confirmed, token_valid) {
        (_, false) => Some("CONFIRMATION_CANCELED"),
        (Some(true), true) => None,
        (Some(false), true) => Some("CONFIRMATION_DECLINED"),
        (None, true) => Some("CONFIRMATION_EXPIRED"),
    }
}

async fn write_outcome<C>(conn: &mut C, outcome: serde_json::Value) -> std::io::Result<()>
where
    C: AsyncWriteExt + Unpin,
{
    write_frame(conn, &BrokerResponse { outcome }).await
}
