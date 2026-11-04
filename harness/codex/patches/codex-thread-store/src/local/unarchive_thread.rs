use super::LocalThreadStore;
use super::helpers::owned_rollout_paths_from_index;
use super::helpers::restore_rollout_moves;
use super::helpers::rollout_path_is_archived;
use super::helpers::scoped_rollout_path;
use super::helpers::touch_modified_time;
use super::helpers::validated_rollout_file_name;
use crate::ArchiveThreadParams;
use crate::ReadThreadParams;
use crate::StoredThread;
use crate::ThreadStoreError;
use crate::ThreadStoreResult;
use codex_protocol::ThreadId;
use codex_rollout::RolloutReferenceIndex;
use codex_rollout::rollout_date_parts;
use tracing::warn;

use super::thread_rollout_resolver;
use super::thread_rollout_resolver::RolloutLocation;

pub(super) async fn unarchive_threads(
    store: &LocalThreadStore,
    params: ArchiveThreadParams,
    descendant_thread_ids: Vec<ThreadId>,
) -> ThreadStoreResult<(StoredThread, Vec<ThreadId>)> {
    let parent_thread_id = params.thread_id;
    let requested_ids: Vec<_> = std::iter::once(parent_thread_id)
        .chain(descendant_thread_ids)
        .collect();
    // Keep the index stable against other mutations in this store, using archive's lock order.
    let mut lock_thread_ids = requested_ids.clone();
    lock_thread_ids.sort_unstable_by_key(ToString::to_string);
    lock_thread_ids.dedup();
    let mut _lifecycle_guards = Vec::with_capacity(lock_thread_ids.len());
    for thread_id in lock_thread_ids {
        _lifecycle_guards.push(store.live_writer_locks.lock_lifecycle(thread_id).await);
    }
    // Hold ownership while discovering and moving rollouts, as archive does. A busy
    // descendant remains a best-effort failure rather than blocking the parent's restore.
    let mut thread_ids = Vec::new();
    let mut _writer_guards = Vec::new();
    for (index, thread_id) in requested_ids.into_iter().enumerate() {
        match store.acquire_writer_lock(thread_id) {
            Ok(guard) => {
                _writer_guards.push(guard);
                thread_ids.push(thread_id);
            }
            Err(err) if index == 0 => return Err(err),
            Err(err) => warn!("failed to unarchive spawned descendant thread {thread_id}: {err}"),
        }
    }
    // Like archive, discover candidate rollouts once for the entire subtree.
    let reference_index = RolloutReferenceIndex::scan_archived_threads(
        store.config.codex_home.as_path(),
        &thread_ids,
    )
    .await
    .map_err(|err| ThreadStoreError::Internal {
        message: format!("failed to scan thread rollout files: {err}"),
    })?;
    unarchive_thread_with_index(store, parent_thread_id, &reference_index)
        .await?
        .ok_or_else(|| ThreadStoreError::InvalidRequest {
            message: format!("no archived rollout found for thread id {parent_thread_id}"),
        })?;
    let root = super::read_thread::read_thread(
        store,
        ReadThreadParams {
            thread_id: parent_thread_id,
            include_archived: false,
            include_history: false,
        },
    )
    .await?;
    let mut restored = vec![parent_thread_id];
    for thread_id in thread_ids.into_iter().skip(1) {
        match unarchive_thread_with_index(store, thread_id, &reference_index).await {
            Ok(Some(thread_id)) => restored.push(thread_id),
            Ok(None) => {}
            Err(err) => warn!("failed to unarchive spawned descendant thread {thread_id}: {err}"),
        }
    }
    Ok((root, restored))
}

async fn unarchive_thread_with_index(
    store: &LocalThreadStore,
    thread_id: ThreadId,
    reference_index: &RolloutReferenceIndex,
) -> ThreadStoreResult<Option<ThreadId>> {
    let state_db_ctx = store.state_db().await;
    let Some(selected_archived_path) =
        thread_rollout_resolver::resolve_current_including_archived(store, thread_id)
            .await?
            .filter(|resolved| resolved.location == RolloutLocation::Archived)
            .map(|resolved| resolved.path)
    else {
        return Ok(None);
    };

    let mut rollout_paths = owned_rollout_paths_from_index(reference_index, thread_id);
    if !rollout_paths.contains(&selected_archived_path) {
        rollout_paths.push(selected_archived_path.clone());
    }
    let mut restored_path = None;
    let mut rollout_moves = Vec::new();
    for rollout_path in rollout_paths {
        if !rollout_path_is_archived(store.config.codex_home.as_path(), rollout_path.as_path()) {
            continue;
        }
        let canonical_archived_path = scoped_rollout_path(
            store
                .config
                .codex_home
                .join(codex_rollout::ARCHIVED_SESSIONS_SUBDIR),
            rollout_path.as_path(),
            "archived",
        )?;
        let file_name =
            validated_rollout_file_name(canonical_archived_path.as_path(), rollout_path.as_path())?;
        let Some((year, month, day)) = rollout_date_parts(&file_name) else {
            return Err(ThreadStoreError::InvalidRequest {
                message: format!(
                    "rollout path `{}` missing filename timestamp",
                    rollout_path.display()
                ),
            });
        };
        let dest_dir = store
            .config
            .codex_home
            .join(codex_rollout::SESSIONS_SUBDIR)
            .join(year)
            .join(month)
            .join(day);
        std::fs::create_dir_all(&dest_dir).map_err(|err| ThreadStoreError::Internal {
            message: format!("failed to unarchive thread: {err}"),
        })?;
        let destination = dest_dir.join(&file_name);
        if rollout_path == selected_archived_path {
            restored_path = Some(destination.clone());
        }
        if !rollout_moves
            .iter()
            .any(|(source, _)| source == &canonical_archived_path)
        {
            rollout_moves.push((canonical_archived_path, destination));
        }
    }
    let restored_path = restored_path.ok_or_else(|| ThreadStoreError::Internal {
        message: format!("failed to unarchive selected rollout for thread {thread_id}"),
    })?;

    for (index, (source, destination)) in rollout_moves.iter().enumerate() {
        if let Err(err) = std::fs::rename(source, destination) {
            if let Err(restore_err) = restore_rollout_moves(&rollout_moves[..index]) {
                return Err(ThreadStoreError::Internal {
                    message: format!(
                        "failed to unarchive thread: {err}; failed to restore moved rollouts: {restore_err}"
                    ),
                });
            }
            return Err(ThreadStoreError::Internal {
                message: format!("failed to unarchive thread: {err}"),
            });
        }
    }
    if let Err(err) = touch_modified_time(restored_path.as_path()) {
        if let Err(restore_err) = restore_rollout_moves(&rollout_moves) {
            return Err(ThreadStoreError::Internal {
                message: format!(
                    "failed to update unarchived thread timestamp: {err}; failed to restore moved rollouts: {restore_err}"
                ),
            });
        }
        return Err(ThreadStoreError::Internal {
            message: format!("failed to update unarchived thread timestamp: {err}"),
        });
    }

    if let Some(ctx) = state_db_ctx
        && let Err(err) = ctx
            .mark_unarchived(thread_id, restored_path.as_path())
            .await
    {
        if let Err(restore_err) = restore_rollout_moves(&rollout_moves) {
            return Err(ThreadStoreError::Internal {
                message: format!(
                    "failed to update unarchived thread metadata: {err}; failed to restore moved rollouts: {restore_err}"
                ),
            });
        }
        return Err(ThreadStoreError::Internal {
            message: format!("failed to update unarchived thread metadata: {err}"),
        });
    }

    // Descendants need only their restored id for notifications, not metadata hydration.
    Ok(Some(thread_id))
}
