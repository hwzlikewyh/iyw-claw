use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

const PREFERENCES_KEY: &str = "app_update_preferences_v1";
static PREFERENCES_WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

impl UpdateChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdatePreferences {
    pub auto_check: bool,
    pub channel: UpdateChannel,
    pub installation_id: String,
    pub last_checked_at: Option<String>,
    pub last_successful_check_at: Option<String>,
    pub failure_count: u32,
    pub skipped_version: Option<String>,
    pub remind_version: Option<String>,
    pub remind_after: Option<String>,
    pub last_offered_release_id: Option<String>,
}

impl Default for UpdatePreferences {
    fn default() -> Self {
        Self {
            auto_check: true,
            channel: UpdateChannel::Stable,
            installation_id: String::new(),
            last_checked_at: None,
            last_successful_check_at: None,
            failure_count: 0,
            skipped_version: None,
            remind_version: None,
            remind_after: None,
            last_offered_release_id: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePreferencesPatch {
    pub auto_check: Option<bool>,
    pub channel: Option<UpdateChannel>,
}

impl UpdatePreferences {
    pub fn ensure_installation_id(&mut self) -> bool {
        if !self.installation_id.trim().is_empty() {
            return false;
        }
        self.installation_id = uuid::Uuid::new_v4().to_string();
        true
    }

    pub fn mark_success(&mut self, release_id: Option<String>) {
        let now = Utc::now().to_rfc3339();
        self.last_checked_at = Some(now.clone());
        self.last_successful_check_at = Some(now);
        self.failure_count = 0;
        self.last_offered_release_id = release_id;
    }

    pub fn mark_failure(&mut self) {
        self.last_checked_at = Some(Utc::now().to_rfc3339());
        self.failure_count = self.failure_count.saturating_add(1);
    }

    pub fn suppresses(&self, version: &str, required: bool) -> bool {
        if required {
            return false;
        }
        if self.skipped_version.as_deref() == Some(version) {
            return true;
        }
        self.remind_version.as_deref() == Some(version)
            && self
                .remind_after
                .as_deref()
                .and_then(parse_time)
                .is_some_and(|value| value > Utc::now())
    }

    pub fn checked_recently(&self, seconds: i64) -> bool {
        self.last_successful_check_at
            .as_deref()
            .and_then(parse_time)
            .is_some_and(|value| Utc::now().signed_duration_since(value).num_seconds() < seconds)
    }

    pub fn reminder_delay(&self) -> Option<std::time::Duration> {
        let deadline = self.remind_after.as_deref().and_then(parse_time)?;
        let millis = deadline
            .signed_duration_since(Utc::now())
            .num_milliseconds();
        (millis > 0).then(|| std::time::Duration::from_millis(millis as u64))
    }

    fn clear_expired_reminder(&mut self) -> Option<bool> {
        if self.remind_version.is_none() && self.remind_after.is_none() {
            return None;
        }
        let valid_version = self
            .remind_version
            .as_deref()
            .is_some_and(|v| !v.is_empty());
        let deadline = self.remind_after.as_deref().and_then(parse_time);
        if valid_version && deadline.as_ref().is_some_and(|value| value > &Utc::now()) {
            return None;
        }
        self.remind_version = None;
        self.remind_after = None;
        Some(valid_version && deadline.is_some())
    }
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

async fn load_unlocked(
    conn: &DatabaseConnection,
) -> Result<(UpdatePreferences, bool), AppCommandError> {
    let raw = app_metadata_service::get_value(conn, PREFERENCES_KEY)
        .await
        .map_err(AppCommandError::from)?;
    let mut preferences = match raw {
        Some(value) => serde_json::from_str(&value).map_err(|error| {
            AppCommandError::configuration_invalid("Failed to parse update preferences")
                .with_detail(error.to_string())
        })?,
        None => UpdatePreferences::default(),
    };
    let dirty = preferences.ensure_installation_id();
    Ok((preferences, dirty))
}

pub async fn load(conn: &DatabaseConnection) -> Result<UpdatePreferences, AppCommandError> {
    let _guard = PREFERENCES_WRITE_LOCK.lock().await;
    let (preferences, dirty) = load_unlocked(conn).await?;
    if dirty {
        save_unlocked(conn, &preferences).await?;
    }
    Ok(preferences)
}

async fn save_unlocked(
    conn: &DatabaseConnection,
    preferences: &UpdatePreferences,
) -> Result<(), AppCommandError> {
    let value = serde_json::to_string(preferences).map_err(|error| {
        AppCommandError::invalid_input("Failed to serialize update preferences")
            .with_detail(error.to_string())
    })?;
    app_metadata_service::upsert_value(conn, PREFERENCES_KEY, &value)
        .await
        .map_err(AppCommandError::from)
}

async fn update_preferences(
    conn: &DatabaseConnection,
    apply: impl FnOnce(&mut UpdatePreferences) -> Result<(), AppCommandError>,
) -> Result<UpdatePreferences, AppCommandError> {
    let _guard = PREFERENCES_WRITE_LOCK.lock().await;
    let (mut preferences, _) = load_unlocked(conn).await?;
    apply(&mut preferences)?;
    save_unlocked(conn, &preferences).await?;
    Ok(preferences)
}

pub async fn patch(
    conn: &DatabaseConnection,
    update: UpdatePreferencesPatch,
) -> Result<UpdatePreferences, AppCommandError> {
    update_preferences(conn, move |preferences| {
        if let Some(value) = update.auto_check {
            preferences.auto_check = value;
        }
        if let Some(value) = update.channel {
            if preferences.channel != value {
                preferences.channel = value;
                preferences.skipped_version = None;
                preferences.remind_version = None;
                preferences.remind_after = None;
                preferences.last_successful_check_at = None;
                preferences.last_offered_release_id = None;
            }
        }
        Ok(())
    })
    .await
}

pub async fn skip_version(
    conn: &DatabaseConnection,
    version: String,
) -> Result<UpdatePreferences, AppCommandError> {
    let version = semver::Version::parse(version.trim()).map_err(|error| {
        AppCommandError::invalid_input("Invalid update version").with_detail(error.to_string())
    })?;
    update_preferences(conn, move |preferences| {
        preferences.skipped_version = Some(version.to_string());
        preferences.remind_version = None;
        preferences.remind_after = None;
        Ok(())
    })
    .await
}

pub async fn remind_later(
    conn: &DatabaseConnection,
    version: String,
    minutes: u32,
) -> Result<UpdatePreferences, AppCommandError> {
    if !(1..=10_080).contains(&minutes) {
        return Err(AppCommandError::invalid_input(
            "Reminder delay must be between 1 minute and 7 days",
        ));
    }
    let version = semver::Version::parse(version.trim()).map_err(|error| {
        AppCommandError::invalid_input("Invalid update version").with_detail(error.to_string())
    })?;
    update_preferences(conn, move |preferences| {
        preferences.skipped_version = None;
        preferences.remind_version = Some(version.to_string());
        preferences.remind_after =
            Some((Utc::now() + chrono::Duration::minutes(i64::from(minutes))).to_rfc3339());
        Ok(())
    })
    .await
}

pub async fn record_check_success(
    conn: &DatabaseConnection,
    channel: UpdateChannel,
    release_id: Option<String>,
) -> Result<(UpdatePreferences, bool), AppCommandError> {
    let _guard = PREFERENCES_WRITE_LOCK.lock().await;
    let (mut preferences, dirty) = load_unlocked(conn).await?;
    let applies = preferences.channel == channel;
    if applies {
        let _ = preferences.clear_expired_reminder();
        preferences.mark_success(release_id);
    }
    if applies || dirty {
        save_unlocked(conn, &preferences).await?;
    }
    Ok((preferences, applies))
}

pub async fn record_check_failure(
    conn: &DatabaseConnection,
    channel: UpdateChannel,
) -> Result<(UpdatePreferences, bool), AppCommandError> {
    let _guard = PREFERENCES_WRITE_LOCK.lock().await;
    let (mut preferences, dirty) = load_unlocked(conn).await?;
    let applies = preferences.channel == channel;
    if applies {
        let _ = preferences.clear_expired_reminder();
        preferences.mark_failure();
    }
    if applies || dirty {
        save_unlocked(conn, &preferences).await?;
    }
    Ok((preferences, applies))
}

pub async fn load_for_scheduler(
    conn: &DatabaseConnection,
) -> Result<(UpdatePreferences, bool), AppCommandError> {
    let _guard = PREFERENCES_WRITE_LOCK.lock().await;
    let (mut preferences, dirty) = load_unlocked(conn).await?;
    let reminder_expired = preferences.clear_expired_reminder();
    if dirty || reminder_expired.is_some() {
        save_unlocked(conn, &preferences).await?;
    }
    Ok((preferences, reminder_expired.unwrap_or(false)))
}
