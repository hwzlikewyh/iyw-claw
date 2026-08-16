use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BrowserErrorCode {
    BrowserUnsupportedRuntime,
    BrowserSidecarMissing,
    BrowserSidecarIntegrityFailed,
    BrowserEngineNotFound,
    BrowserProfileLocked,
    BrowserRuntimeStartTimeout,
    BrowserRuntimeUnavailable,
    BrowserShuttingDown,
    BrowserTabLimit,
    BrowserInvalidArgument,
    BrowserTabNotFound,
    BrowserTabGone,
    BrowserTabCrashed,
    BrowserNavigationFailed,
    BrowserStreamDisconnected,
    BrowserFrameDecodeFailed,
    BrowserStaleGeneration,
    BrowserViewConflict,
    BrowserViewClaimTimeout,
    BrowserControlChanged,
    BrowserUserActive,
    BrowserUserHeld,
    BrowserSnapshotStale,
    BrowserDialogPending,
    BrowserOperationTimeout,
    BrowserCancelled,
    BrowserUploadCancelled,
    BrowserDownloadFailed,
    BrowserInternal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserErrorContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_tab_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_epoch: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
#[serde(rename_all = "camelCase")]
pub struct BrowserError {
    pub code: BrowserErrorCode,
    pub message: String,
    pub retryable: bool,
    pub effect_may_have_occurred: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<BrowserErrorContext>,
}

impl BrowserError {
    pub fn new(code: BrowserErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
            effect_may_have_occurred: false,
            context: None,
        }
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    pub fn effect_may_have_occurred(mut self, value: bool) -> Self {
        self.effect_may_have_occurred = value;
        self
    }

    pub fn with_context(mut self, context: BrowserErrorContext) -> Self {
        self.context = Some(context);
        self
    }

    pub fn tab_not_found(tab_id: &str) -> Self {
        Self::new(
            BrowserErrorCode::BrowserTabNotFound,
            "The browser tab does not exist",
        )
        .with_context(BrowserErrorContext {
            browser_tab_id: Some(tab_id.to_string()),
            ..BrowserErrorContext::default()
        })
    }

    pub fn stale_generation(context: BrowserErrorContext) -> Self {
        Self::new(
            BrowserErrorCode::BrowserStaleGeneration,
            "The browser operation belongs to an obsolete state generation",
        )
        .with_context(context)
    }

    pub fn shutting_down() -> Self {
        Self::new(
            BrowserErrorCode::BrowserShuttingDown,
            "The browser runtime is shutting down",
        )
    }
}
