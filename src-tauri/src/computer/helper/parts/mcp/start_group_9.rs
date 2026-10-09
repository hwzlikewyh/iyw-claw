// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl McpClient {
    /// Start reading `reader` in the background and return a client that
    /// writes to `writer`.
    pub fn start<R, W>(reader: R, writer: W) -> Arc<Self>
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let closed = Arc::new(watch::Sender::new(false));
        let closed_tx = closed.clone();
        let client = Arc::new(Self {
            writer: tokio::sync::Mutex::new(Box::new(writer)),
            pending: pending.clone(),
            next_id: AtomicU64::new(1),
            closed,
            turn: tokio::sync::Mutex::new(()),
        });
        let weak = Arc::downgrade(&client);
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match read_line_bounded(&mut lines, &mut buf).await {
                    Ok(true) => {}
                    Ok(false) | Err(_) => break,
                }
                let Ok(message) = serde_json::from_slice::<Value>(&buf) else {
                    continue;
                };
                let id = message.get("id").and_then(Value::as_u64);
                let is_reply = message.get("result").is_some() || message.get("error").is_some();
                match (id, is_reply) {
                    (Some(id), true) => {
                        let tx = pending
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .remove(&id);
                        if let Some(tx) = tx {
                            let _ = tx.send(reply_value(message));
                        }
                    }
                    // A request from the driver to us. Answer it so it is not
                    // left pending; the helper offers nothing.
                    (_, false)
                        if message.get("method").is_some() && message.get("id").is_some() =>
                    {
                        if let Some(client) = weak.upgrade() {
                            let answer = json!({
                                "jsonrpc": "2.0",
                                "id": message["id"].clone(),
                                "error": { "code": -32601, "message": "method not found" },
                            });
                            let _ = client.write_message(&answer).await;
                        }
                    }
                    // Notifications (progress, logging): nothing to do.
                    _ => {}
                }
            }
            closed_tx.send_replace(true);
            fail_pending(&pending);
        });
        client
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    /// Give up on the driver: every call waiting on it is told so, and
    /// [`is_closed`](Self::is_closed) says so from now on.
    pub fn close(&self) {
        self.closed.send_replace(true);
        fail_pending(&self.pending);
    }

    pub(in crate::computer::helper::mcp) async fn write_message(
        &self,
        message: &Value,
    ) -> Result<(), McpError> {
        let mut line =
            serde_json::to_vec(message).map_err(|e| McpError::Protocol(e.to_string()))?;
        line.push(b'\n');
        let write = async {
            let mut writer = self.writer.lock().await;
            writer.write_all(&line).await?;
            writer.flush().await
        };
        match tokio::time::timeout(WRITE_TIMEOUT, write).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(McpError::Closed),
            Err(_) => {
                self.close();
                Err(McpError::Timeout)
            }
        }
    }

    /// One call, in turn with every other. `timeout` runs from when this
    /// call's turn comes.
    pub(in crate::computer::helper::mcp) async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, McpError> {
        let _turn = self.turn.lock().await;
        if self.is_closed() {
            return Err(McpError::Closed);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, tx);
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if let Err(e) = self.write_message(&message).await {
            self.pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => Err(McpError::Closed),
            Err(_) => {
                // Still at work on it, and it answers nothing else until it
                // is done: give up on this driver rather than queue behind.
                self.close();
                Err(McpError::Timeout)
            }
        }
    }

    /// The MCP handshake. Must complete before any tool call.
    pub async fn initialize(&self, timeout: Duration) -> Result<Value, McpError> {
        let result = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "iyw-computer-helper", "version": env!("CARGO_PKG_VERSION") },
                }),
                timeout,
            )
            .await?;
        self.write_message(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await?;
        Ok(result)
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<ToolCallResult, McpError> {
        let result = self
            .request(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
                timeout,
            )
            .await?;
        ToolCallResult::from_value(result)
    }
}
