use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use super::dto::CapabilityPolicySnapshot;
use super::error::CapabilityPolicyError;

pub const CACHE_KEY: &str = "agent_capability_policy.snapshot.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CachedCapabilityPolicy {
    pub snapshot: CapabilityPolicySnapshot,
    pub etag: Option<String>,
}

impl CachedCapabilityPolicy {
    pub fn validate(&self) -> Result<(), CapabilityPolicyError> {
        self.snapshot.validate_structure()?;
        validate_etag(self.etag.as_deref())
    }
}

#[async_trait]
pub trait CapabilityPolicyCache: Send + Sync {
    async fn load(&self) -> Result<Option<CachedCapabilityPolicy>, CapabilityPolicyError>;
    async fn save(&self, value: &CachedCapabilityPolicy) -> Result<(), CapabilityPolicyError>;
}

#[derive(Clone)]
pub struct AppMetadataPolicyCache {
    conn: DatabaseConnection,
}

impl AppMetadataPolicyCache {
    pub fn new(conn: DatabaseConnection) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl CapabilityPolicyCache for AppMetadataPolicyCache {
    async fn load(&self) -> Result<Option<CachedCapabilityPolicy>, CapabilityPolicyError> {
        let raw = crate::db::service::app_metadata_service::get_value(&self.conn, CACHE_KEY)
            .await
            .map_err(CapabilityPolicyError::cache)?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let value = serde_json::from_str::<CachedCapabilityPolicy>(&raw)
            .map_err(CapabilityPolicyError::cache)?;
        value.validate()?;
        Ok(Some(value))
    }

    async fn save(&self, value: &CachedCapabilityPolicy) -> Result<(), CapabilityPolicyError> {
        value.validate()?;
        let raw = serde_json::to_string(value).map_err(CapabilityPolicyError::cache)?;
        crate::db::service::app_metadata_service::upsert_value(&self.conn, CACHE_KEY, &raw)
            .await
            .map_err(CapabilityPolicyError::cache)
    }
}

pub(crate) fn normalize_etag(
    value: Option<String>,
) -> Result<Option<String>, CapabilityPolicyError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 256 || !value.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(CapabilityPolicyError::InvalidSnapshot(
            "capability policy ETag is invalid".into(),
        ));
    }
    Ok(Some(value.to_string()))
}

fn validate_etag(value: Option<&str>) -> Result<(), CapabilityPolicyError> {
    normalize_etag(value.map(ToString::to_string)).map(|_| ())
}
