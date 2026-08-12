use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

const COMMAND_QUEUE_CAPACITY: usize = 16;

#[derive(Clone, Default)]
pub struct RealtimeVoiceState {
    sessions: Arc<Mutex<HashMap<String, SessionEntry>>>,
}

struct SessionEntry {
    session_id: String,
    sender: Option<mpsc::Sender<SessionCommand>>,
    finishing: bool,
}

pub(super) enum SessionCommand {
    Audio(Vec<u8>),
    Finish,
    Cancel,
}

impl RealtimeVoiceState {
    pub(super) async fn reserve(&self, window: &str, session_id: &str) -> Result<(), ()> {
        let mut sessions = self.sessions.lock().await;
        if sessions.contains_key(window) {
            return Err(());
        }
        sessions.insert(
            window.to_string(),
            SessionEntry {
                session_id: session_id.to_string(),
                sender: None,
                finishing: false,
            },
        );
        Ok(())
    }

    pub(super) async fn activate(
        &self,
        window: &str,
        session_id: &str,
    ) -> Option<mpsc::Receiver<SessionCommand>> {
        let mut sessions = self.sessions.lock().await;
        let entry = sessions.get_mut(window)?;
        if entry.session_id != session_id || entry.sender.is_some() {
            return None;
        }
        let (sender, receiver) = mpsc::channel(COMMAND_QUEUE_CAPACITY);
        entry.sender = Some(sender);
        Some(receiver)
    }

    pub(super) async fn sender(
        &self,
        window: &str,
        session_id: &str,
    ) -> Option<mpsc::Sender<SessionCommand>> {
        let sessions = self.sessions.lock().await;
        let entry = sessions.get(window)?;
        if entry.session_id != session_id || entry.finishing {
            return None;
        }
        entry.sender.clone()
    }

    pub(super) async fn begin_finish(
        &self,
        window: &str,
        session_id: &str,
    ) -> Option<mpsc::Sender<SessionCommand>> {
        let mut sessions = self.sessions.lock().await;
        let entry = sessions.get_mut(window)?;
        if entry.session_id != session_id || entry.finishing {
            return None;
        }
        entry.finishing = true;
        entry.sender.clone()
    }

    pub(super) async fn remove(
        &self,
        window: &str,
        session_id: &str,
    ) -> Option<mpsc::Sender<SessionCommand>> {
        let mut sessions = self.sessions.lock().await;
        let matches = sessions
            .get(window)
            .is_some_and(|entry| entry.session_id == session_id);
        matches
            .then(|| sessions.remove(window).and_then(|entry| entry.sender))
            .flatten()
    }

    pub async fn cancel_window(&self, window: &str) {
        let sender = self
            .sessions
            .lock()
            .await
            .remove(window)
            .and_then(|entry| entry.sender);
        if let Some(sender) = sender {
            let _ = sender.send(SessionCommand::Cancel).await;
        }
    }
}
