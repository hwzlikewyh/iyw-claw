mod client;
mod session;
mod state;

use std::time::Duration;

use futures_util::SinkExt;
use serde::Serialize;
use tauri::ipc::Channel;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::app_error::AppCommandError;
use crate::commands::iyw_account::iyw_account_access_token_core;
use crate::db::AppDatabase;

pub use state::RealtimeVoiceState;
use state::SessionCommand;

const MAX_AUDIO_CHUNK_BYTES: usize = 12_800;
const COMMAND_SEND_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct LocalSession {
    window: String,
    id: String,
}

struct PreparedSession {
    state: RealtimeVoiceState,
    local: LocalSession,
    socket: client::VoiceSocket,
    commands: mpsc::Receiver<SessionCommand>,
}

struct TranscriptionSegment {
    sequence: u64,
    text: String,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RealtimeVoiceEvent {
    Ready {
        session_id: String,
    },
    Partial {
        session_id: String,
        sequence: u64,
        text: String,
        start_ms: Option<u64>,
        end_ms: Option<u64>,
    },
    Final {
        session_id: String,
        sequence: u64,
        text: String,
        start_ms: Option<u64>,
        end_ms: Option<u64>,
    },
    Completed {
        session_id: String,
        duration_ms: Option<u64>,
    },
    Error {
        session_id: String,
        code: String,
        message: String,
    },
}

impl RealtimeVoiceEvent {
    fn ready(session_id: &str) -> Self {
        Self::Ready {
            session_id: session_id.to_string(),
        }
    }

    fn partial(session_id: &str, segment: TranscriptionSegment) -> Self {
        Self::Partial {
            session_id: session_id.to_string(),
            sequence: segment.sequence,
            text: segment.text,
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
        }
    }

    fn final_text(session_id: &str, segment: TranscriptionSegment) -> Self {
        Self::Final {
            session_id: session_id.to_string(),
            sequence: segment.sequence,
            text: segment.text,
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
        }
    }

    fn completed(session_id: &str, duration_ms: Option<u64>) -> Self {
        Self::Completed {
            session_id: session_id.to_string(),
            duration_ms,
        }
    }

    fn error(session_id: &str, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            session_id: session_id.to_string(),
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeVoiceStartResult {
    session_id: String,
}

#[tauri::command]
pub async fn realtime_voice_start(
    window: tauri::WebviewWindow,
    db: tauri::State<'_, AppDatabase>,
    state: tauri::State<'_, RealtimeVoiceState>,
    on_event: Channel<RealtimeVoiceEvent>,
) -> Result<RealtimeVoiceStartResult, AppCommandError> {
    let token = iyw_account_access_token_core(&db.conn)
        .await?
        .ok_or_else(|| {
            AppCommandError::authentication_failed("Sign in before using voice input")
        })?;
    let local = LocalSession {
        window: window.label().to_string(),
        id: Uuid::new_v4().to_string(),
    };
    let prepared = prepare_session(state.inner(), local, token.expose()).await?;
    let session_id = prepared.local.id.clone();
    prepared.launch(on_event).await?;
    Ok(RealtimeVoiceStartResult { session_id })
}

async fn prepare_session(
    state: &RealtimeVoiceState,
    local: LocalSession,
    token: &str,
) -> Result<PreparedSession, AppCommandError> {
    state
        .reserve(&local.window, &local.id)
        .await
        .map_err(|_| AppCommandError::already_exists("Voice input is already active"))?;
    let socket = match client::connect(token).await {
        Ok(socket) => socket,
        Err(error) => {
            state.remove(&local.window, &local.id).await;
            return Err(error);
        }
    };
    let Some(commands) = state.activate(&local.window, &local.id).await else {
        close_failed_session(state, &local, socket).await;
        return Err(AppCommandError::task_execution_failed(
            "Voice input session could not be activated",
        ));
    };
    Ok(PreparedSession {
        state: state.clone(),
        local,
        socket,
        commands,
    })
}

async fn close_failed_session(
    state: &RealtimeVoiceState,
    local: &LocalSession,
    mut socket: client::VoiceSocket,
) {
    let _ = socket.close(None).await;
    state.remove(&local.window, &local.id).await;
}

impl PreparedSession {
    async fn launch(self, events: Channel<RealtimeVoiceEvent>) -> Result<(), AppCommandError> {
        if events
            .send(RealtimeVoiceEvent::ready(&self.local.id))
            .is_err()
        {
            close_failed_session(&self.state, &self.local, self.socket).await;
            return Err(AppCommandError::task_execution_failed(
                "Voice input event channel is unavailable",
            ));
        }
        let session_id = self.local.id.clone();
        let window = self.local.window.clone();
        let state = self.state.clone();
        tauri::async_runtime::spawn(async move {
            session::SessionRuntime::new(session_id.clone(), self.socket, events)
                .run(self.commands)
                .await;
            state.remove(&window, &session_id).await;
        });
        tracing::info!(session_id = %self.local.id, window = %self.local.window, "[RealtimeVoice] session started");
        Ok(())
    }
}

#[tauri::command]
pub async fn realtime_voice_push_audio(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, RealtimeVoiceState>,
    session_id: String,
    chunk: Vec<u8>,
) -> Result<(), AppCommandError> {
    if chunk.is_empty() || chunk.len() > MAX_AUDIO_CHUNK_BYTES || chunk.len() % 2 != 0 {
        return Err(AppCommandError::invalid_input(
            "Invalid realtime voice audio chunk",
        ));
    }
    let sender = state
        .sender(window.label(), &session_id)
        .await
        .ok_or_else(|| AppCommandError::not_found("Voice input session was not found"))?;
    sender
        .try_send(SessionCommand::Audio(chunk))
        .map_err(|error| {
            AppCommandError::task_execution_failed("Voice input audio queue is unavailable")
                .with_detail(error.to_string())
        })
}

#[tauri::command]
pub async fn realtime_voice_finish(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, RealtimeVoiceState>,
    session_id: String,
) -> Result<(), AppCommandError> {
    let sender = state
        .begin_finish(window.label(), &session_id)
        .await
        .ok_or_else(|| AppCommandError::not_found("Voice input session was not found"))?;
    send_command(sender, SessionCommand::Finish).await
}

#[tauri::command]
pub async fn realtime_voice_cancel(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, RealtimeVoiceState>,
    session_id: String,
) -> Result<(), AppCommandError> {
    let Some(sender) = state.remove(window.label(), &session_id).await else {
        return Ok(());
    };
    send_command(sender, SessionCommand::Cancel).await
}

async fn send_command(
    sender: tokio::sync::mpsc::Sender<SessionCommand>,
    command: SessionCommand,
) -> Result<(), AppCommandError> {
    tokio::time::timeout(COMMAND_SEND_TIMEOUT, sender.send(command))
        .await
        .map_err(|_| AppCommandError::task_execution_failed("Voice input command timed out"))?
        .map_err(|error| {
            AppCommandError::task_execution_failed("Voice input session is unavailable")
                .with_detail(error.to_string())
        })
}
