use sea_orm::DatabaseConnection;
use serde_json::json;

use crate::app_error::AppCommandError;

use super::{
    direct_upload, routing_description, SkillMarketAddVersionRequest, SkillMarketDetail,
    SkillMarketPublishRequest,
};

pub async fn publish_core(
    conn: &DatabaseConnection,
    request: SkillMarketPublishRequest,
) -> Result<SkillMarketDetail, AppCommandError> {
    routing_description::validate_routing_descriptions(&request.files, request.package_type)?;
    let mut metadata = json!({
        "slug": request.slug, "displayName": request.display_name,
        "summary": request.summary, "category": request.category,
        "iconUrl": request.icon_url.unwrap_or_default(), "tags": request.tags,
        "visibility": request.visibility, "version": request.version,
        "changelog": request.changelog, "packageType": request.package_type,
        "dependencies": request.dependencies
    });
    if let Some(audience) = request.audience {
        metadata["audience"] = json!(audience);
    }
    direct_upload::publish(conn, metadata, request.files).await
}

pub async fn add_version_core(
    conn: &DatabaseConnection,
    request: SkillMarketAddVersionRequest,
) -> Result<SkillMarketDetail, AppCommandError> {
    routing_description::validate_routing_descriptions(&request.files, request.package_type)?;
    let metadata = json!({
        "skillId": super::types::parse_id(&request.id)?.to_string(),
        "version": request.version, "changelog": request.changelog,
        "packageType": request.package_type, "dependencies": request.dependencies
    });
    direct_upload::publish(conn, metadata, request.files).await
}
