use super::feedback_thread_index::FeedbackThreadIndex;
use super::*;
use crate::error_code::OVERLOADED_ERROR_CODE;
use codex_connectors::ConnectorDirectoryCacheContext;
use codex_connectors::ConnectorDirectoryCacheKey;
use codex_connectors::connector_runtime_cache_path;
use codex_feedback::CODEX_APP_DIRECTORY_CACHE_ATTACHMENT_FILENAME;
use codex_feedback::CODEX_APPS_TOOLS_CACHE_ATTACHMENT_FILENAME;
#[cfg(target_os = "windows")]
use codex_feedback::WINDOWS_SANDBOX_LOG_ATTACHMENT_FILENAME;
use codex_feedback::guardian_review_failures;
use codex_rollout::RolloutRecorder;
use sha2::Digest;
use sha2::Sha256;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub(crate) struct FeedbackRequestProcessor {
    auth_manager: Arc<AuthManager>,
    thread_manager: Arc<ThreadManager>,
    config: Arc<Config>,
    feedback: CodexFeedback,
    log_db: Option<LogDbLayer>,
    state_db: Option<StateDbHandle>,
    uploads: Arc<Semaphore>,
}

impl FeedbackRequestProcessor {
    pub(crate) fn new(
        auth_manager: Arc<AuthManager>,
        thread_manager: Arc<ThreadManager>,
        config: Arc<Config>,
        feedback: CodexFeedback,
        log_db: Option<LogDbLayer>,
        state_db: Option<StateDbHandle>,
    ) -> Self {
        Self {
            auth_manager,
            thread_manager,
            config,
            feedback,
            log_db,
            state_db,
            uploads: Arc::new(Semaphore::new(/*permits*/ 3)),
        }
    }

    pub(crate) async fn feedback_upload(
        &self,
        params: FeedbackUploadParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.upload_feedback_response(params)
            .await
            .map(|response| Some(response.into()))
    }

    async fn upload_feedback_response(
        &self,
        params: FeedbackUploadParams,
    ) -> Result<FeedbackUploadResponse, JSONRPCErrorError> {
        if !self.config.feedback_enabled {
            return Err(invalid_request(
                "sending feedback is disabled by configuration",
            ));
        }
        let permit = self
            .uploads
            .clone()
            .try_acquire_owned()
            .map_err(|_| JSONRPCErrorError {
                code: OVERLOADED_ERROR_CODE,
                message:
                    "Three feedback uploads are already in progress; try again after one finishes"
                        .to_string(),
                data: None,
            })?;

        let FeedbackUploadParams {
            classification,
            reason,
            thread_id,
            include_logs,
            extra_log_files,
            tags,
        } = params;
        let mut upload_tags = tags.unwrap_or_default();

        let conversation_id = match thread_id.as_deref() {
            Some(thread_id) => match ThreadId::from_string(thread_id) {
                Ok(conversation_id) => Some(conversation_id),
                Err(err) => return Err(invalid_request(format!("invalid thread id: {err}"))),
            },
            None => None,
        };

        let auth = self.auth_manager.auth_cached();
        let turn_metadata = if let Some(conversation_id) = conversation_id
            && let Some(rollout_path) = self
                .resolve_rollout_path(conversation_id, self.state_db.as_ref())
                .await
        {
            feedback_turn_metadata_from_rollout(
                &rollout_path,
                upload_tags.get("turn_id").map(String::as_str),
            )
            .await
        } else {
            None
        };
        apply_feedback_turn_metadata(&mut upload_tags, turn_metadata);

        if let Some(chatgpt_user_id) = auth
            .as_ref()
            .and_then(codex_login::CodexAuth::get_chatgpt_user_id)
        {
            tracing::info!(target: "feedback_tags", chatgpt_user_id);
        }
        if let Some(account_id) = auth
            .as_ref()
            .and_then(codex_login::CodexAuth::get_account_id)
        {
            tracing::info!(target: "feedback_tags", account_id);
        }
        let snapshot = self.feedback.snapshot(conversation_id);
        let thread_id = snapshot.thread_id.clone();
        let mut extra_attachments = Vec::new();
        let mut feedback_index = None;
        let (sqlite_feedback_logs, state_db_ctx) = if include_logs {
            if let Some(log_db) = self.log_db.as_ref() {
                log_db.flush().await;
            }
            let state_db_ctx = self.state_db.clone();
            let feedback_thread_ids = match conversation_id {
                Some(conversation_id) => match self
                    .thread_manager
                    .list_agent_subtree_thread_ids(conversation_id)
                    .await
                {
                    Ok(thread_ids) => thread_ids,
                    Err(err) => {
                        warn!(
                            "failed to list feedback subtree for thread_id={conversation_id}: {err}"
                        );
                        vec![conversation_id]
                    }
                },
                None => Vec::new(),
            };
            let failures = guardian_review_failures(&feedback_thread_ids);
            let mut feedback_thread_ids = feedback_thread_ids;
            if let Some(conversation_id) = conversation_id {
                let index =
                    FeedbackThreadIndex::new(conversation_id, feedback_thread_ids, &failures);
                feedback_thread_ids = index
                    .threads
                    .iter()
                    .map(|thread| thread.thread_id)
                    .collect();
                feedback_index = Some(index);
            }
            extra_attachments.extend(failures.attachment);
            let sqlite_feedback_logs = if let Some(state_db_ctx) = state_db_ctx.as_ref()
                && !feedback_thread_ids.is_empty()
            {
                let thread_id_texts = feedback_thread_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                let thread_id_refs = thread_id_texts
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                match state_db_ctx
                    .query_feedback_logs_for_threads(&thread_id_refs)
                    .await
                {
                    Ok(logs) if logs.is_empty() => None,
                    Ok(logs) => Some(logs),
                    Err(err) => {
                        let thread_ids = thread_id_texts.join(", ");
                        warn!(
                            "failed to query feedback logs from sqlite for thread_ids=[{thread_ids}]: {err}"
                        );
                        None
                    }
                }
            } else {
                None
            };
            (sqlite_feedback_logs, state_db_ctx)
        } else {
            (None, None)
        };

        let mut attachment_paths = Vec::new();
        let mut seen_attachment_paths = HashSet::new();
        // Keep actor/reviewer pairs together: reported thread, recent failed-review
        // children, then newest remaining children. Captured failures precede these files.
        if include_logs {
            for thread in feedback_index
                .iter_mut()
                .flat_map(|index| &mut index.threads)
            {
                if let Some(rollout_path) = self
                    .resolve_rollout_path(thread.thread_id, state_db_ctx.as_ref())
                    .await
                    && seen_attachment_paths.insert(rollout_path.clone())
                {
                    thread.rollout_filename = rollout_path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned());
                    attachment_paths.push(FeedbackAttachmentPath {
                        path: rollout_path,
                        attachment_filename_override: None,
                    });
                }
                if let Ok(conversation) = self.thread_manager.get_thread(thread.thread_id).await
                    && let Some(guardian_rollout_path) =
                        conversation.guardian_trunk_rollout_path().await
                    && seen_attachment_paths.insert(guardian_rollout_path.clone())
                {
                    let filename = auto_review_rollout_filename(thread.thread_id);
                    thread.guardian_rollout_filename = Some(filename.clone());
                    attachment_paths.push(FeedbackAttachmentPath {
                        path: guardian_rollout_path,
                        attachment_filename_override: Some(filename),
                    });
                }
            }
            if let Some(index) = feedback_index {
                match index.attachment() {
                    Ok(attachment) => extra_attachments.insert(0, attachment),
                    Err(err) => warn!("failed to serialize feedback thread index: {err}"),
                }
            }
            if let Some(sandbox_log_attachment) =
                windows_sandbox_log_attachment(&self.config.codex_home)
                && seen_attachment_paths.insert(sandbox_log_attachment.path.clone())
            {
                attachment_paths.push(sandbox_log_attachment);
            }
            for cache_attachment in tool_cache_feedback_attachments(
                self.config.codex_home.as_path(),
                &self.config.chatgpt_base_url,
                auth.as_ref(),
            ) {
                if seen_attachment_paths.insert(cache_attachment.path.clone()) {
                    attachment_paths.push(cache_attachment);
                }
            }
        }
        if let Some(extra_log_files) = extra_log_files {
            for extra_log_file in extra_log_files {
                if seen_attachment_paths.insert(extra_log_file.clone()) {
                    attachment_paths.push(FeedbackAttachmentPath {
                        path: extra_log_file,
                        attachment_filename_override: None,
                    });
                }
            }
        }

        if include_logs {
            let doctor_cwd = feedback_cwd(
                &self.thread_manager,
                self.state_db.as_ref(),
                conversation_id,
                self.config.cwd.as_path(),
            )
            .await;
            if let Some(doctor_report) =
                super::feedback_doctor_report::doctor_feedback_report(&self.config, &doctor_cwd)
                    .await
            {
                extra_attachments.push(doctor_report.attachment);
                for (key, value) in doctor_report.tags {
                    upload_tags.entry(key).or_insert(value);
                }
            }
        }

        let session_source = self.thread_manager.session_source();
        let http_client_factory = self.config.http_client_factory();
        let runtime_handle = tokio::runtime::Handle::current();

        let upload_result = tokio::task::spawn_blocking(move || {
            // Cancelling the RPC waiter must not release a still-running upload's slot.
            let _permit = permit;
            let tags = (!upload_tags.is_empty()).then_some(&upload_tags);
            runtime_handle.block_on(snapshot.upload_feedback(
                FeedbackUploadOptions {
                    classification: &classification,
                    reason: reason.as_deref(),
                    tags,
                    include_logs,
                    extra_attachments: &extra_attachments,
                    extra_attachment_paths: &attachment_paths,
                    session_source: Some(session_source),
                    logs_override: sqlite_feedback_logs,
                },
                &http_client_factory,
            ))
        })
        .await;

        let upload_result = match upload_result {
            Ok(result) => result,
            Err(join_err) => {
                return Err(internal_error(format!(
                    "failed to upload feedback: {join_err}"
                )));
            }
        };

        upload_result
            .map_err(|err| internal_error(format!("failed to upload feedback: {err:#}")))?;
        Ok(FeedbackUploadResponse { thread_id })
    }

    async fn resolve_rollout_path(
        &self,
        conversation_id: ThreadId,
        state_db_ctx: Option<&StateDbHandle>,
    ) -> Option<PathBuf> {
        if let Ok(conversation) = self.thread_manager.get_thread(conversation_id).await
            && let Some(rollout_path) = conversation.rollout_path()
        {
            return Some(rollout_path);
        }

        let state_db_ctx = state_db_ctx?;
        state_db_ctx
            .find_rollout_path_by_id(conversation_id, /*archived_only*/ None)
            .await
            .unwrap_or_else(|err| {
                warn!("failed to resolve rollout path for thread_id={conversation_id}: {err}");
                None
            })
    }
}

async fn feedback_cwd(
    thread_manager: &ThreadManager,
    state_db: Option<&StateDbHandle>,
    conversation_id: Option<ThreadId>,
    fallback_cwd: &Path,
) -> PathBuf {
    let Some(conversation_id) = conversation_id else {
        return fallback_cwd.to_path_buf();
    };

    if let Ok(conversation) = thread_manager.get_thread(conversation_id).await {
        return conversation.config_snapshot().await.cwd().to_path_buf();
    }

    let Some(state_db) = state_db else {
        return fallback_cwd.to_path_buf();
    };
    match state_db.get_thread(conversation_id).await {
        Ok(Some(metadata)) => metadata.cwd,
        Ok(None) => fallback_cwd.to_path_buf(),
        Err(err) => {
            warn!("failed to resolve cwd for feedback thread_id={conversation_id}: {err}");
            fallback_cwd.to_path_buf()
        }
    }
}

#[derive(Debug, PartialEq)]
struct FeedbackTurnMetadata {
    model: String,
    effort: Option<ReasoningEffort>,
    prompt_hash: Option<String>,
}

fn apply_feedback_turn_metadata(
    upload_tags: &mut BTreeMap<String, String>,
    turn_metadata: Option<FeedbackTurnMetadata>,
) {
    // These are reserved tags derived from the persisted rollout rather than
    // accepted from the feedback request.
    upload_tags.remove("prompt_hash");
    upload_tags.remove("prompt_version");

    if let Some(FeedbackTurnMetadata {
        model,
        effort,
        prompt_hash,
    }) = turn_metadata
    {
        upload_tags.insert("model".to_string(), model);
        upload_tags.insert("effort".to_string(), format!("{effort:?}"));
        if let Some(prompt_hash) = prompt_hash {
            upload_tags.insert("prompt_hash".to_string(), prompt_hash);
        }
    }
}

async fn feedback_turn_metadata_from_rollout(
    rollout_path: &Path,
    turn_id: Option<&str>,
) -> Option<FeedbackTurnMetadata> {
    let (items, _, _) = RolloutRecorder::load_rollout_items(rollout_path)
        .await
        .ok()?;
    let prompt_hash = items.iter().find_map(|item| match item {
        RolloutItem::SessionMeta(meta) => meta
            .meta
            .base_instructions
            .as_ref()
            .map(|prompt| normalized_prompt_hash(&prompt.text)),
        _ => None,
    });

    items.into_iter().rev().find_map(|item| match item {
        RolloutItem::TurnContext(context)
            if turn_id.is_none() || context.turn_id.as_deref() == turn_id =>
        {
            Some(FeedbackTurnMetadata {
                model: context.model,
                effort: context.effort,
                prompt_hash: prompt_hash.clone(),
            })
        }
        _ => None,
    })
}

fn normalized_prompt_hash(prompt: &str) -> String {
    let normalized_prompt = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("{:x}", Sha256::digest(normalized_prompt.as_bytes()))
}

fn tool_cache_feedback_attachments(
    codex_home: &Path,
    chatgpt_base_url: &str,
    auth: Option<&CodexAuth>,
) -> Vec<FeedbackAttachmentPath> {
    let mut attachments = Vec::with_capacity(2);
    let tools_cache_path = connector_runtime_cache_path(codex_home, auth);
    if tools_cache_path.is_file() {
        attachments.push(FeedbackAttachmentPath {
            path: tools_cache_path,
            attachment_filename_override: Some(
                CODEX_APPS_TOOLS_CACHE_ATTACHMENT_FILENAME.to_string(),
            ),
        });
    }

    let Some(auth) = auth.filter(|auth| auth.uses_codex_backend()) else {
        return attachments;
    };
    let directory_cache_context = ConnectorDirectoryCacheContext::new(
        codex_home.to_path_buf(),
        ConnectorDirectoryCacheKey::new(
            chatgpt_base_url.to_string(),
            auth.get_account_id(),
            auth.get_chatgpt_user_id(),
            auth.is_workspace_account(),
        ),
    );
    let directory_cache_path = directory_cache_context.cache_path();
    if directory_cache_path.is_file() {
        attachments.push(FeedbackAttachmentPath {
            path: directory_cache_path,
            attachment_filename_override: Some(
                CODEX_APP_DIRECTORY_CACHE_ATTACHMENT_FILENAME.to_string(),
            ),
        });
    }

    attachments
}

fn auto_review_rollout_filename(thread_id: ThreadId) -> String {
    format!("auto-review-rollout-{thread_id}.jsonl")
}

#[cfg(target_os = "windows")]
fn windows_sandbox_log_attachment(codex_home: &Path) -> Option<FeedbackAttachmentPath> {
    let sandbox_log_path = codex_windows_sandbox::current_log_file_path_for_codex_home(codex_home);
    sandbox_log_path
        .is_file()
        .then_some(FeedbackAttachmentPath {
            path: sandbox_log_path,
            attachment_filename_override: Some(WINDOWS_SANDBOX_LOG_ATTACHMENT_FILENAME.to_string()),
        })
}

#[cfg(not(target_os = "windows"))]
fn windows_sandbox_log_attachment(_codex_home: &Path) -> Option<FeedbackAttachmentPath> {
    None
}
