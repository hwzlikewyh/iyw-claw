#[cfg(feature = "tauri-runtime")]
use std::time::Duration;

use serde::{Deserialize, Serialize};
#[cfg(feature = "tauri-runtime")]
use tauri_plugin_updater::{Update, Updater, UpdaterExt};

#[cfg(feature = "tauri-runtime")]
use crate::app_error::AppCommandError;
use crate::update::preferences::{UpdateChannel, UpdatePreferences};
#[cfg(feature = "tauri-runtime")]
use crate::update::state::{self as update_state, AppUpdateStateHandle};
#[cfg(feature = "tauri-runtime")]
use crate::web::event_bridge::EventEmitter;

pub const UPDATE_BASE_URL_ENV: &str = "IYW_CLAW_UPDATE_BASE_URL";

#[cfg(debug_assertions)]
const DEFAULT_UPDATE_BASE_URL: &str = "http://127.0.0.1:6001";
#[cfg(not(debug_assertions))]
const DEFAULT_UPDATE_BASE_URL: &str = "https://gateway.iyw.cn/iyw-fusion-api";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckReason {
    Automatic,
    Manual,
}

impl CheckReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReleaseExtensions {
    #[serde(default, alias = "releaseId")]
    pub release_id: Option<String>,
    #[serde(default)]
    pub channel: Option<UpdateChannel>,
    #[serde(default, alias = "updatePolicy")]
    pub update_policy: Option<UpdatePolicy>,
    #[serde(default, alias = "enforceAfter")]
    pub enforce_after: Option<String>,
    #[serde(default, alias = "rolloutPercent")]
    pub rollout_percent: Option<f64>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePolicy {
    Optional,
    Required,
}

impl UpdatePolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Optional => "optional",
            Self::Required => "required",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    pub version: String,
    pub body: String,
    pub date: Option<String>,
    pub release_id: Option<String>,
    pub channel: String,
    pub update_policy: String,
    pub enforce_after: Option<String>,
    pub rollout_percent: Option<f64>,
    pub size: Option<u64>,
    pub sha256: Option<String>,
}

pub fn configured_base_url() -> String {
    std::env::var(UPDATE_BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_UPDATE_BASE_URL.to_string())
}

pub fn endpoint(preferences: &UpdatePreferences, reason: CheckReason) -> String {
    format!(
        "{}/app-updates/v1/check/{{{{target}}}}/{{{{arch}}}}/{{{{current_version}}}}?product=iyw-claw&runtime=desktop&channel={}&reason={}",
        configured_base_url(),
        preferences.channel.as_str(),
        reason.as_str()
    )
}

#[cfg(feature = "tauri-runtime")]
pub fn desktop_updater(
    app: &tauri::AppHandle,
    preferences: &UpdatePreferences,
    reason: CheckReason,
) -> Result<Updater, AppCommandError> {
    let endpoint = endpoint(preferences, reason)
        .parse::<tauri::Url>()
        .map_err(|error| {
            AppCommandError::configuration_invalid("Invalid update endpoint")
                .with_detail(error.to_string())
        })?;
    app.updater_builder()
        .endpoints(vec![endpoint])
        .map_err(updater_error)?
        .header("X-IYW-Installation-ID", &preferences.installation_id)
        .map_err(updater_error)?
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(updater_error)
}

#[cfg(feature = "tauri-runtime")]
pub async fn check_desktop_update(
    app: &tauri::AppHandle,
    preferences: &UpdatePreferences,
    reason: CheckReason,
) -> Result<Option<AppUpdateInfo>, AppCommandError> {
    let Some(update) = desktop_updater(app, preferences, reason)?
        .check()
        .await
        .map_err(updater_error)?
    else {
        return Ok(None);
    };
    let extensions = validated_extensions(&update, preferences)?;
    Ok(Some(update_info(&update, &extensions, preferences.channel)))
}

/// Re-check, download, and install the update while reporting shared progress.
/// Windows NSIS exits from `Installing`; other platforms stage a restart.
#[cfg(feature = "tauri-runtime")]
pub async fn download_and_install(
    app: &tauri::AppHandle,
    preferences: &UpdatePreferences,
    handle: AppUpdateStateHandle,
    emitter: EventEmitter,
) -> Result<(), String> {
    let update = desktop_updater(app, preferences, CheckReason::Manual)
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No update available".to_string())?;
    validated_extensions(&update, preferences).map_err(|error| error.to_string())?;
    let version = update.version.clone();
    update_state::set_download_target(&handle, &emitter, version.clone());
    let progress = std::sync::Arc::new(update_state::ProgressEmitter::new(
        handle.clone(),
        emitter.clone(),
    ));
    install_update(InstallContext {
        update,
        version,
        handle,
        emitter,
        progress,
    })
    .await
}

#[cfg(feature = "tauri-runtime")]
struct InstallContext {
    update: Update,
    version: String,
    handle: AppUpdateStateHandle,
    emitter: EventEmitter,
    progress: std::sync::Arc<update_state::ProgressEmitter>,
}

#[cfg(all(feature = "tauri-runtime", target_os = "windows"))]
async fn install_update(context: InstallContext) -> Result<(), String> {
    let chunk_progress = context.progress.clone();
    let finish_progress = context.progress;
    let mut downloaded = 0_u64;
    let bytes = context
        .update
        .download(
            move |chunk_len, content_len| {
                downloaded += chunk_len as u64;
                chunk_progress.downloading(downloaded, content_len);
            },
            move || finish_progress.verifying(),
        )
        .await
        .map_err(|error| error.to_string())?;
    update_state::set_installing(&context.handle, &context.emitter);
    tracing::info!(
        version = %context.version,
        "[app-update] launching NSIS updater; the desktop process will exit"
    );
    context
        .update
        .install(bytes)
        .map_err(|error| error.to_string())?;
    Err("NSIS updater returned without terminating the application".to_string())
}

#[cfg(all(feature = "tauri-runtime", not(target_os = "windows")))]
async fn install_update(context: InstallContext) -> Result<(), String> {
    let chunk_progress = context.progress.clone();
    let finish_progress = context.progress;
    let mut downloaded = 0_u64;
    context
        .update
        .download_and_install(
            move |chunk_len, content_len| {
                downloaded += chunk_len as u64;
                chunk_progress.downloading(downloaded, content_len);
            },
            move || finish_progress.verifying(),
        )
        .await
        .map_err(|error| error.to_string())?;
    update_state::set_ready(
        &context.handle,
        &context.emitter,
        Some(context.version),
        None,
        None,
        None,
    );
    Ok(())
}

#[cfg(feature = "tauri-runtime")]
pub fn extensions(update: &Update) -> Result<ReleaseExtensions, AppCommandError> {
    serde_json::from_value(update.raw_json.clone()).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid application update metadata")
            .with_detail(error.to_string())
    })
}

#[cfg(feature = "tauri-runtime")]
fn validated_extensions(
    update: &Update,
    preferences: &UpdatePreferences,
) -> Result<ReleaseExtensions, AppCommandError> {
    let extensions = extensions(update)?;
    if extensions
        .channel
        .is_some_and(|value| value != preferences.channel)
    {
        return Err(AppCommandError::configuration_invalid(
            "Application update channel does not match the request",
        ));
    }
    Ok(extensions)
}

#[cfg(feature = "tauri-runtime")]
pub fn update_info(
    update: &Update,
    ext: &ReleaseExtensions,
    fallback_channel: UpdateChannel,
) -> AppUpdateInfo {
    AppUpdateInfo {
        version: update.version.clone(),
        body: update.body.clone().unwrap_or_default(),
        date: update.date.as_ref().map(ToString::to_string),
        release_id: ext.release_id.clone(),
        channel: ext.channel.unwrap_or(fallback_channel).as_str().to_string(),
        update_policy: ext
            .update_policy
            .unwrap_or(UpdatePolicy::Optional)
            .as_str()
            .to_string(),
        enforce_after: ext.enforce_after.clone(),
        rollout_percent: ext.rollout_percent,
        size: ext.size,
        sha256: ext.sha256.clone(),
    }
}

#[cfg(feature = "tauri-runtime")]
pub(crate) fn updater_error(error: tauri_plugin_updater::Error) -> AppCommandError {
    AppCommandError::network("Application update request failed").with_detail(error.to_string())
}
