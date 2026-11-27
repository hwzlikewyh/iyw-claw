use std::time::Duration;

use sea_orm::DatabaseConnection;
use serde::Deserialize;

use super::preferences::{self, UpdateChannel, UpdatePreferences};

pub(super) fn accepts_channel(
    settings: &UpdatePreferences,
    channel: &str,
    release: Option<&str>,
) -> bool {
    settings.channel.as_str() == channel
        || (settings.channel == UpdateChannel::Beta && channel == "stable")
        || (settings.channel_source == "installer"
            && channel == "beta"
            && release.is_some()
            && settings.promoted_release_id.as_deref() == release)
}

#[derive(Deserialize)]
struct Candidate {
    #[serde(alias = "releaseId")]
    release_id: String,
    channel: UpdateChannel,
}

#[derive(Deserialize)]
struct Envelope {
    code: i32,
    data: Option<Candidate>,
}

pub async fn synchronize(
    conn: &DatabaseConnection,
    preferences: UpdatePreferences,
) -> UpdatePreferences {
    let id = crate::brand::release_id();
    if id.is_empty()
        || preferences.channel_source != "installer"
        || preferences.promoted_release_id.as_deref() == Some(id)
    {
        return preferences;
    }
    match fetch_stable(&preferences).await {
        Ok(Some(candidate)) if candidate.channel == UpdateChannel::Stable => {
            if candidate.release_id != id {
                return preferences;
            }
            let channel = if crate::brand::promotion_channel() == "beta" {
                UpdateChannel::Beta
            } else {
                UpdateChannel::Stable
            };
            match apply_promotion(conn, id, channel).await {
                Ok(updated) => return updated,
                Err(error) => tracing::warn!(release_id = id, %error, "保存测试版晋级状态失败"),
            }
        }
        Ok(_) => {}
        Err(error) => tracing::debug!(release_id = id, %error, "暂未同步到正式发布状态"),
    }
    preferences
}

async fn fetch_stable(
    preferences: &UpdatePreferences,
) -> Result<Option<Candidate>, reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let arch = if cfg!(target_arch = "x86") {
        "i686"
    } else {
        std::env::consts::ARCH
    };
    // 复用现有版本查询，不需要新增品牌或候选接口。
    let response: Envelope = client
        .get(format!(
            "{}/app-updates/v1/check/{}/{arch}/0.0.0",
            super::release::configured_base_url(),
            crate::distribution_platform::TARGET
        ))
        .query(&[
            ("product", crate::brand::PRODUCT),
            ("brandId", crate::brand::id()),
            ("runtime", "desktop"),
            ("channel", "stable"),
            ("reason", "manual"),
        ])
        .header("X-IYW-Installation-ID", &preferences.installation_id)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(if response.code == 1 {
        response.data
    } else {
        None
    })
}

async fn apply_promotion(
    conn: &DatabaseConnection,
    release_id: &str,
    channel: UpdateChannel,
) -> Result<UpdatePreferences, crate::app_error::AppCommandError> {
    preferences::update_preferences(conn, |preferences| {
        if preferences.channel_source == "installer"
            && preferences.promoted_release_id.as_deref() != Some(release_id)
        {
            preferences.channel = channel;
            preferences.promoted_release_id = Some(release_id.to_string());
            preferences.skipped_version = None;
            preferences.remind_version = None;
            preferences.remind_after = None;
            preferences.last_successful_check_at = None;
        }
        Ok(())
    })
    .await
}
