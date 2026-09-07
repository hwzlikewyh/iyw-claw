use super::*;
use crate::acp::interactive_html::{
    HtmlResponse, HtmlResponseAction, InteractiveHtmlRequest, InteractiveHtmlState, RegisteredHtml,
    MAX_OPEN_PAGES,
};

pub(super) struct HtmlEntry {
    pub parent_connection_id: String,
    pub waiting: bool,
    pub sender: Option<tokio::sync::oneshot::Sender<serde_json::Value>>,
    cancellation: tokio_util::sync::CancellationToken,
}

impl Drop for HtmlEntry {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

impl ConnectionManager {
    pub async fn present_html(
        &self,
        conn_id: &str,
        request: InteractiveHtmlRequest,
    ) -> Result<RegisteredHtml, String> {
        request.validate()?;
        let (state, emitter) = self
            .get_state_and_emitter(conn_id)
            .await
            .ok_or("HTML interaction session is disconnected")?;
        let interaction = InteractiveHtmlState {
            interaction_id: uuid::Uuid::new_v4().to_string(),
            title: request.title.trim().to_string(),
            html: request.html,
            wait_for_response: request.wait_for_response,
            cancellation: tokio_util::sync::CancellationToken::new(),
        };
        let registered = self.register_html_entry(conn_id, &interaction).await?;
        emit_with_state(
            &state,
            &emitter,
            AcpEvent::InteractiveHtmlPresented {
                interaction: interaction.clone(),
            },
        )
        .await;
        self.reconcile_html_presentation(&interaction, &state, &emitter)
            .await;
        tracing::info!(connection_id = %conn_id, interaction_id = %registered.interaction_id,
            waiting = request.wait_for_response, html_bytes = interaction.html.len(),
            "interactive HTML presented");
        Ok(registered)
    }

    async fn reconcile_html_presentation(
        &self,
        interaction: &InteractiveHtmlState,
        state: &Arc<tokio::sync::RwLock<crate::acp::SessionState>>,
        emitter: &EventEmitter,
    ) {
        // 关闭可能先于展示广播，补发关闭以防客户端残留无接收器的页面。
        if !self
            .interactive_html
            .lock()
            .await
            .contains_key(&interaction.interaction_id)
        {
            emit_with_state(
                state,
                emitter,
                AcpEvent::InteractiveHtmlClosed {
                    interaction_id: interaction.interaction_id.clone(),
                },
            )
            .await;
        }
    }

    async fn register_html_entry(
        &self,
        conn_id: &str,
        interaction: &InteractiveHtmlState,
    ) -> Result<RegisteredHtml, String> {
        // 与 register_question 保持相同锁顺序，两个工具共享唯一待回答槽位。
        let mut pages = self.interactive_html.lock().await;
        let questions = self.pending_questions.lock().await;
        let own_pages = pages
            .values()
            .filter(|entry| entry.parent_connection_id == conn_id);
        if own_pages.clone().count() >= MAX_OPEN_PAGES {
            return Err("Close an existing HTML page before opening another".to_string());
        }
        if interaction.wait_for_response
            && (own_pages.clone().any(|entry| entry.waiting)
                || questions
                    .values()
                    .any(|entry| entry.parent_connection_id == conn_id))
        {
            return Err("This session already has an interaction awaiting the user".to_string());
        }
        let (entry, registered) = new_html_entry(conn_id, interaction);
        pages.insert(interaction.interaction_id.clone(), entry);
        Ok(registered)
    }

    pub async fn respond_html(
        &self,
        conn_id: &str,
        response: HtmlResponse,
    ) -> Result<(), AcpError> {
        response.validate().map_err(AcpError::protocol)?;
        let mut pages = self.interactive_html.lock().await;
        let entry = pages
            .get(&response.interaction_id)
            .ok_or_else(|| AcpError::protocol("HTML interaction is no longer active"))?;
        if entry.parent_connection_id != conn_id {
            return Err(AcpError::protocol(
                "HTML interaction belongs to another session",
            ));
        }
        if !entry.waiting && !matches!(response.action, HtmlResponseAction::Close) {
            return Err(AcpError::protocol(
                "This HTML page does not collect responses",
            ));
        }
        let mut entry = pages
            .remove(&response.interaction_id)
            .expect("entry verified under lock");
        drop(pages);
        if let Some(sender) = entry.sender.take() {
            let _ = sender.send(response.outcome());
        }
        self.emit_html_closed(conn_id, &response.interaction_id)
            .await;
        tracing::info!(connection_id = %conn_id, interaction_id = %response.interaction_id,
            action = ?response.action, "interactive HTML resolved");
        Ok(())
    }

    pub async fn cancel_html(&self, conn_id: &str, interaction_id: &str) {
        let mut pages = self.interactive_html.lock().await;
        if !pages
            .get(interaction_id)
            .is_some_and(|entry| entry.parent_connection_id == conn_id)
        {
            return;
        }
        pages.remove(interaction_id);
        drop(pages);
        self.emit_html_closed(conn_id, interaction_id).await;
        tracing::info!(connection_id = %conn_id, interaction_id, "interactive HTML closed");
    }

    pub async fn cancel_html_by_parent(&self, conn_id: &str) {
        let mut pages = self.interactive_html.lock().await;
        let ids: Vec<_> = pages
            .iter()
            .filter(|(_, entry)| entry.parent_connection_id == conn_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ids {
            pages.remove(id);
        }
        drop(pages);
        for id in ids {
            self.emit_html_closed(conn_id, &id).await;
        }
    }

    async fn emit_html_closed(&self, conn_id: &str, interaction_id: &str) {
        if let Some((state, emitter)) = self.get_state_and_emitter(conn_id).await {
            emit_with_state(
                &state,
                &emitter,
                AcpEvent::InteractiveHtmlClosed {
                    interaction_id: interaction_id.to_string(),
                },
            )
            .await;
        }
    }
}

fn new_html_entry(
    conn_id: &str,
    interaction: &InteractiveHtmlState,
) -> (HtmlEntry, RegisteredHtml) {
    let (sender, answer_rx) = if interaction.wait_for_response {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };
    let entry = HtmlEntry {
        parent_connection_id: conn_id.to_string(),
        waiting: interaction.wait_for_response,
        sender,
        cancellation: interaction.cancellation.clone(),
    };
    let registered = RegisteredHtml {
        interaction_id: interaction.interaction_id.clone(),
        answer_rx,
        cancellation: interaction.cancellation.clone(),
    };
    (entry, registered)
}
