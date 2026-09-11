use std::collections::HashSet;

use serde_json::{json, Value};

use super::{UpstreamClient, UpstreamError};

// 宿主取消请求的等待窗口为 8 秒，留出 ACP 回复和退出清理时间。
const CANCEL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

impl UpstreamClient {
    pub(crate) async fn cancel_owned_tree(&self, root: &str) -> Result<(), UpstreamError> {
        tokio::time::timeout(CANCEL_TIMEOUT, async {
            self.pause_active_goal(root).await?;
            self.interrupt_if_running(root).await?;
            loop {
                let mut interrupted = false;
                for thread in self.loaded_threads().await? {
                    if thread == root || !self.bind_loaded_descendant(&thread).await? {
                        continue;
                    }
                    if !self.descendant_of(&thread, root).await {
                        continue;
                    }
                    self.synchronize_child_turn(&thread).await?;
                    interrupted |= self.interrupt_if_running(&thread).await?;
                }
                // 停止子线程期间可能新建孙线程，再读一次稳定集合。
                if !interrupted {
                    return Ok(());
                }
            }
        })
        .await
        .map_err(|_| UpstreamError::Io("星河停止所属回合超时，需要关闭运行器".into()))?
    }

    async fn interrupt_if_running(&self, thread: &str) -> Result<bool, UpstreamError> {
        let Some(turn) = self.active_turn_for(thread).await else {
            return Ok(false);
        };
        match self.interrupt_turn_for_thread(thread).await {
            Ok(_) => {}
            Err(UpstreamError::Rpc {
                code: -32600,
                message,
            }) if message == "no active turn to interrupt" => {}
            Err(error) => return Err(error),
        }
        // 有 ID 的 interrupt 仅在 TurnAborted 后回复；此时可退役迟到事件。
        self.complete_turn_for_thread(thread, &turn.turn_id).await?;
        Ok(true)
    }

    async fn pause_active_goal(&self, thread: &str) -> Result<(), UpstreamError> {
        let response = match self.request_json_for_thread(thread, json!({
            "method": "thread/goal/get", "params": {"threadId": thread}
        })).await {
            Err(UpstreamError::Rpc {message, ..}) if message == "goals feature is disabled" => return Ok(()),
            result => result?,
        };
        if response.pointer("/goal/status").and_then(Value::as_str) == Some("active") {
            self.request_json_for_thread(thread, json!({
                "method": "thread/goal/set", "params": {"threadId": thread, "status": "paused"}
            })).await?;
        }
        Ok(())
    }

    async fn bind_loaded_descendant(&self, thread: &str) -> Result<bool, UpstreamError> {
        match self.bind_descendant(thread).await {
            Ok(()) => Ok(true),
            Err(UpstreamError::InvalidRequest(message))
                if message == "thread is not an owned subagent" =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    async fn loaded_threads(&self) -> Result<Vec<String>, UpstreamError> {
        let mut cursor = Value::Null;
        let mut visited = HashSet::new();
        let mut threads = Vec::new();
        loop {
            let page = self.send(json!({ "method": "thread/loaded/list", "params": { "cursor": cursor, "limit": 100 } })).await?;
            let data = page["data"]
                .as_array()
                .ok_or_else(|| invalid("loaded thread response has no data"))?;
            for thread in data {
                threads.push(
                    thread
                        .as_str()
                        .ok_or_else(|| invalid("loaded thread has an invalid id"))?
                        .to_string(),
                );
            }
            let Some(next) = page["nextCursor"].as_str() else {
                return Ok(threads);
            };
            if !visited.insert(next.to_string()) {
                return Err(invalid("loaded thread cursor repeated"));
            }
            cursor = json!(next);
        }
    }
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidResponse(message.into())
}
