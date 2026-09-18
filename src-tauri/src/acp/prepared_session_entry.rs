use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::acp::error::AcpError;
use crate::acp::prepared_session::{PrepareSessionRequest, PreparedSessionHandle};
use crate::web::event_bridge::EventEmitter;

pub(super) const MAX_PREPARED_SESSIONS: usize = 2;
pub(super) const PREPARATION_LIFETIME: Duration = Duration::from_secs(120);

#[derive(Clone, Default)]
pub(in crate::acp::manager) struct Progress {
    pub fingerprint: Option<String>,
    pub registered: bool,
    pub ready: bool,
    pub error: Option<String>,
    pub retired: bool,
}

pub(in crate::acp::manager) struct Entry {
    pub id: String,
    pub request: PrepareSessionRequest,
    pub owner: String,
    pub working_dir: String,
    pub owns_workspace: bool,
    pub workspace_reserved: AtomicBool,
    pub created: Instant,
    pub claimed: Arc<AtomicBool>,
    pub cancel: CancellationToken,
    pub progress: watch::Sender<Progress>,
    pub emitter: EventEmitter,
}

impl Entry {
    pub(super) fn new(
        mut request: PrepareSessionRequest,
        context: (String, String, bool),
        emitter: &EventEmitter,
    ) -> Self {
        let (owner, working_dir, owns_workspace) = context;
        request.working_dir = Some(working_dir.clone());
        let (emitter, claimed) = emitter.prepared();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            request,
            owner,
            working_dir,
            owns_workspace,
            workspace_reserved: AtomicBool::new(false),
            created: Instant::now(),
            claimed,
            cancel: CancellationToken::new(),
            progress: watch::channel(Progress::default()).0,
            emitter,
        }
    }

    pub(super) fn handle(&self) -> PreparedSessionHandle {
        PreparedSessionHandle {
            id: self.id.clone(),
            working_dir: self.working_dir.clone(),
        }
    }

    pub(super) fn matches(&self, request: &PrepareSessionRequest, owner: &str) -> bool {
        self.same_target(request, owner)
            && !self.cancel.is_cancelled()
            && self.created.elapsed() < PREPARATION_LIFETIME
            && self.request.preferred_mode_id == request.preferred_mode_id
            && self.request.preferred_config_values == request.preferred_config_values
    }

    pub(super) fn same_target(&self, request: &PrepareSessionRequest, owner: &str) -> bool {
        self.owner == owner
            && self.request.agent_type == request.agent_type
            && self.request.session_id == request.session_id
            && self.request.conversation_id == request.conversation_id
            && request
                .working_dir
                .as_ref()
                .map_or(self.owns_workspace, |cwd| cwd == &self.working_dir)
    }

    pub(in crate::acp::manager) fn registered(&self) {
        self.progress
            .send_modify(|progress| progress.registered = true);
    }

    pub(super) fn fail(&self, error: &AcpError) {
        self.progress
            .send_modify(|progress| progress.error = Some(error.to_string()));
    }

    pub(super) async fn wait_ready(&self) -> Result<String, AcpError> {
        let mut progress = self.progress.subscribe();
        loop {
            let value = progress.borrow_and_update().clone();
            if let Some(error) = value.error {
                return Err(AcpError::protocol(error));
            }
            if value.registered && value.ready {
                return value
                    .fingerprint
                    .ok_or_else(|| AcpError::protocol("Prepared session has no fingerprint"));
            }
            tokio::select! {
                _ = self.cancel.cancelled() => return Err(AcpError::protocol("Session preparation cancelled")),
                result = progress.changed() => result.map_err(|_| AcpError::protocol("Session preparation closed"))?,
            }
        }
    }

    pub(super) async fn ready_for(
        &self,
        request: &PrepareSessionRequest,
        owner: &str,
    ) -> Result<Option<String>, AcpError> {
        if !self.matches(request, owner) {
            self.cancel.cancel();
            self.wait_retired().await?;
            return Ok(None);
        }
        match self.wait_ready().await {
            Ok(fingerprint) => Ok(Some(fingerprint)),
            Err(_) => {
                self.wait_retired().await?;
                Ok(None)
            }
        }
    }

    pub(super) async fn wait_retired(&self) -> Result<(), AcpError> {
        let mut progress = self.progress.subscribe();
        tokio::time::timeout(PREPARATION_LIFETIME, async {
            loop {
                if progress.borrow_and_update().retired {
                    return Ok(());
                }
                progress
                    .changed()
                    .await
                    .map_err(|_| AcpError::protocol("Preparation cleanup closed"))?;
            }
        })
        .await
        .map_err(|_| AcpError::protocol("Previous prepared session is still shutting down"))?
    }

    pub(super) fn is_claimed(&self) -> bool {
        self.claimed.load(Ordering::Acquire)
    }
}
