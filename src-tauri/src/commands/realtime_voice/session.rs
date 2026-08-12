use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::client::{self, GatewayEvent, VoiceSocket};
use super::state::SessionCommand;
use super::{RealtimeVoiceEvent, TranscriptionSegment};

const STOP_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) struct SessionRuntime {
    session_id: String,
    socket: VoiceSocket,
    events: tauri::ipc::Channel<RealtimeVoiceEvent>,
}

struct EventSink {
    session_id: String,
    events: tauri::ipc::Channel<RealtimeVoiceEvent>,
}

struct CommandContext<'a, 'b> {
    socket: &'a mut VoiceSocket,
    finishing: &'a mut bool,
    stop_timeout: &'a mut std::pin::Pin<&'b mut tokio::time::Sleep>,
    sink: &'a EventSink,
}

impl SessionRuntime {
    pub(super) fn new(
        session_id: String,
        socket: VoiceSocket,
        events: tauri::ipc::Channel<RealtimeVoiceEvent>,
    ) -> Self {
        Self {
            session_id,
            socket,
            events,
        }
    }

    pub(super) async fn run(self, mut commands: mpsc::Receiver<SessionCommand>) {
        let stop_timeout = tokio::time::sleep(Duration::from_secs(86_400));
        tokio::pin!(stop_timeout);
        let mut socket = self.socket;
        let sink = EventSink::new(self.session_id, self.events);
        let mut finishing = false;
        loop {
            tokio::select! {
                command = commands.recv() => {
                    let context = CommandContext {
                        socket: &mut socket,
                        finishing: &mut finishing,
                        stop_timeout: &mut stop_timeout,
                        sink: &sink,
                    };
                    if !handle_command(command, context).await { break; }
                }
                incoming = socket.next() => {
                    if !handle_incoming(incoming, &sink).await { break; }
                }
                _ = &mut stop_timeout, if finishing => {
                    sink.error("VOICE_COMPLETION_TIMEOUT", "Voice transcription did not finish in time.");
                    break;
                }
            }
        }
        let _ = socket.close(None).await;
        tracing::info!(session_id = %sink.session_id, "[RealtimeVoice] session closed");
    }
}

async fn handle_command(command: Option<SessionCommand>, context: CommandContext<'_, '_>) -> bool {
    let result = match command {
        Some(SessionCommand::Audio(bytes)) if !*context.finishing => {
            context.socket.send(Message::Binary(bytes.into())).await
        }
        Some(SessionCommand::Finish) if !*context.finishing => {
            *context.finishing = true;
            context
                .stop_timeout
                .as_mut()
                .reset(tokio::time::Instant::now() + STOP_TIMEOUT);
            context
                .socket
                .send(Message::Text(r#"{"type":"finish"}"#.into()))
                .await
        }
        Some(SessionCommand::Audio(_)) | Some(SessionCommand::Finish) => return true,
        Some(SessionCommand::Cancel) | None => return false,
    };
    if let Err(error) = result {
        tracing::warn!(error = %error, "[RealtimeVoice] failed to send WebSocket frame");
        context
            .sink
            .error("VOICE_SEND_FAILED", "Voice audio could not be sent.");
        return false;
    }
    true
}

async fn handle_incoming(
    incoming: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
    sink: &EventSink,
) -> bool {
    let message = match incoming {
        Some(Ok(message)) => message,
        Some(Err(error)) => {
            tracing::warn!(error = %error, "[RealtimeVoice] WebSocket receive failed");
            sink.error("VOICE_CONNECTION_LOST", "Voice connection was lost.");
            return false;
        }
        None => {
            sink.error("VOICE_CONNECTION_CLOSED", "Voice connection closed.");
            return false;
        }
    };
    if matches!(message, Message::Close(_)) {
        sink.error("VOICE_CONNECTION_CLOSED", "Voice connection closed.");
        return false;
    }
    let event = match client::parse_event(message) {
        Ok(Some(event)) => event,
        Ok(None) => return true,
        Err(detail) => {
            tracing::warn!(%detail, "[RealtimeVoice] invalid gateway event");
            sink.error(
                "VOICE_INVALID_RESPONSE",
                "Voice service returned an invalid response.",
            );
            return false;
        }
    };
    forward_gateway_event(sink, event)
}

fn forward_gateway_event(sink: &EventSink, event: GatewayEvent) -> bool {
    let Some((outgoing, terminal)) = gateway_event(sink, event) else {
        return true;
    };
    if sink.events.send(outgoing).is_err() {
        return false;
    }
    !terminal
}

fn gateway_event(sink: &EventSink, event: GatewayEvent) -> Option<(RealtimeVoiceEvent, bool)> {
    let result = match event {
        GatewayEvent::Partial {
            sequence,
            text,
            start_ms,
            end_ms,
        } => {
            let segment = TranscriptionSegment {
                sequence,
                text,
                start_ms,
                end_ms,
            };
            (segment_event(sink, false, segment), false)
        }
        GatewayEvent::Final {
            sequence,
            text,
            start_ms,
            end_ms,
        } => {
            let segment = TranscriptionSegment {
                sequence,
                text,
                start_ms,
                end_ms,
            };
            (segment_event(sink, true, segment), false)
        }
        GatewayEvent::Completed { duration_ms } => (
            RealtimeVoiceEvent::completed(&sink.session_id, duration_ms),
            true,
        ),
        GatewayEvent::Error { code, message } => (
            RealtimeVoiceEvent::error(&sink.session_id, code, message),
            true,
        ),
        GatewayEvent::Ready { .. } => return None,
        GatewayEvent::Unknown => {
            tracing::debug!("[RealtimeVoice] ignored unknown gateway event");
            return None;
        }
    };
    Some(result)
}

fn segment_event(
    sink: &EventSink,
    final_text: bool,
    segment: TranscriptionSegment,
) -> RealtimeVoiceEvent {
    if final_text {
        RealtimeVoiceEvent::final_text(&sink.session_id, segment)
    } else {
        RealtimeVoiceEvent::partial(&sink.session_id, segment)
    }
}

impl EventSink {
    fn new(session_id: String, events: tauri::ipc::Channel<RealtimeVoiceEvent>) -> Self {
        Self { session_id, events }
    }

    fn error(&self, code: &str, message: &str) {
        let _ = self
            .events
            .send(RealtimeVoiceEvent::error(&self.session_id, code, message));
    }
}
