use serde_json::{json, Value};

use super::{UpstreamClient, UpstreamError};

impl UpstreamClient {
    /// Legacy rollout 不支持分页；只在明确 method-not-found 时回退官方历史读取。
    pub(crate) async fn history_page(
        &self,
        thread: &str,
        request: Value,
    ) -> Result<Value, UpstreamError> {
        match self.request_json_for_thread(thread, request.clone()).await {
            Err(UpstreamError::Rpc { code: -32601, .. }) => {
                self.legacy_history_page(thread, &request).await
            }
            result => result,
        }
    }

    async fn legacy_history_page(
        &self,
        thread: &str,
        request: &Value,
    ) -> Result<Value, UpstreamError> {
        let snapshot = self
            .request_json_for_thread(
                thread,
                json!({
                    "method": "thread/read", "params": { "threadId": thread, "includeTurns": true },
                }),
            )
            .await?;
        let turns = snapshot
            .pointer("/thread/turns")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("legacy history has no turns"))?;
        let mut data = match request["method"].as_str() {
            Some("thread/turns/list") => turns.clone(),
            Some("thread/items/list") => {
                let turn = request
                    .pointer("/params/turnId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("item history has no turn"))?;
                let value = turns
                    .iter()
                    .find(|value| value["id"] == turn)
                    .ok_or_else(|| invalid("legacy history is missing the requested turn"))?;
                let items = value["items"]
                    .as_array()
                    .ok_or_else(|| invalid("legacy turn has no items"))?;
                items
                    .iter()
                    .map(|item| json!({ "turnId": turn, "item": item }))
                    .collect()
            }
            _ => return Err(invalid("unsupported legacy history method")),
        };
        if request
            .pointer("/params/sortDirection")
            .and_then(Value::as_str)
            == Some("desc")
        {
            data.reverse();
        }
        Ok(json!({ "data": data, "nextCursor": null }))
    }
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidResponse(message.into())
}
