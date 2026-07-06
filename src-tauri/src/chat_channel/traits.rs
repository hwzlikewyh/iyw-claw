use async_trait::async_trait;
use tokio::sync::mpsc;

use super::error::ChatChannelError;
use super::types::*;

#[async_trait]
pub trait ChatChannelBackend: Send + Sync + 'static {
    fn channel_type(&self) -> ChannelType;

    /// Start the receiving loop. `command_tx` forwards incoming IM messages
    /// to the central command dispatcher.
    async fn start(
        &self,
        command_tx: mpsc::Sender<IncomingCommand>,
    ) -> Result<(), ChatChannelError>;

    /// Stop the backend connection gracefully.
    async fn stop(&self) -> Result<(), ChatChannelError>;

    /// Current connection status.
    async fn status(&self) -> ChannelConnectionStatus;

    /// Send a plain text message.
    async fn send_message(&self, text: &str) -> Result<SentMessageId, ChatChannelError>;

    /// Send a rich/structured message (Telegram Markdown / Lark Card).
    async fn send_rich_message(
        &self,
        message: &RichMessage,
    ) -> Result<SentMessageId, ChatChannelError>;

    /// [Phase 2] Send an interactive message with action buttons.
    /// Default implementation degrades to send_rich_message.
    async fn send_interactive_message(
        &self,
        message: &InteractiveMessage,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.send_rich_message(&message.to_rich_fallback()).await
    }

    /// [Phase 2] Update an already-sent message (e.g., permission status change).
    async fn update_message(
        &self,
        _message_id: &SentMessageId,
        _message: &RichMessage,
    ) -> Result<(), ChatChannelError> {
        Ok(())
    }

    /// Test the connection (used by "Test Connection" button in UI).
    async fn test_connection(&self) -> Result<(), ChatChannelError>;
}
