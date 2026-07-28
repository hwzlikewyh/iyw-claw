use crate::update::state::{
    AppUpdateLifecycle, AppUpdateState, AppUpdateStateHandle, APP_UPDATE_STATE_CHANNEL,
};
use crate::web::event_bridge::{emit_event, EventEmitter};

#[derive(Debug, Clone)]
pub struct Identity {
    pub version: String,
    pub release_id: Option<String>,
    pub channel: String,
}

/// Atomically capture and claim the exact desktop offer that the user accepted.
/// Offer metadata stays in the snapshot so required updates remain enforceable.
pub fn try_begin(
    handle: &AppUpdateStateHandle,
    emitter: &EventEmitter,
) -> (bool, AppUpdateState, Option<Identity>) {
    let (snapshot, offer) = {
        let mut state = handle.write().unwrap_or_else(|error| error.into_inner());
        if !matches!(
            state.status,
            AppUpdateLifecycle::Available | AppUpdateLifecycle::Error
        ) {
            return (false, state.clone(), None);
        }
        let (Some(version), Some(channel)) = (state.version.clone(), state.channel.clone()) else {
            return (false, state.clone(), None);
        };
        let offer = Identity {
            version,
            release_id: state.release_id.clone(),
            channel,
        };
        state.seq += 1;
        state.status = AppUpdateLifecycle::Downloading;
        state.downloaded = Some(0);
        state.total = None;
        state.error = None;
        (state.clone(), offer)
    };
    emit_event(emitter, APP_UPDATE_STATE_CHANNEL, &snapshot);
    (true, snapshot, Some(offer))
}

pub fn is_current_optional(handle: &AppUpdateStateHandle, version: &str) -> bool {
    let state = handle.read().unwrap_or_else(|error| error.into_inner());
    state.status == AppUpdateLifecycle::Available
        && state.update_policy.as_deref() == Some("optional")
        && state.version.as_deref() == Some(version)
}

#[cfg(feature = "tauri-runtime")]
pub fn validate_checked_update(
    update: &tauri_plugin_updater::Update,
    extensions: &crate::update::release::ReleaseExtensions,
    expected: &Identity,
) -> Result<(), String> {
    let channel = extensions
        .channel
        .map(|value| value.as_str())
        .unwrap_or(expected.channel.as_str());
    if update.version != expected.version
        || extensions.release_id.as_deref() != expected.release_id.as_deref()
        || channel != expected.channel
    {
        return Err("The available update changed before installation; check again".to_string());
    }
    Ok(())
}
