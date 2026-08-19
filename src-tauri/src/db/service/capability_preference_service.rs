use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
};

use crate::db::entities::capability_preference;
use crate::db::error::DbError;

const MAX_KEY_CHARS: usize = 512;

#[derive(Debug, Clone)]
pub struct CapabilityPreferenceInput {
    pub subject_kind: String,
    pub subject_id: String,
    pub capability: String,
    pub enabled: bool,
}

pub async fn list_for_subject(
    conn: &DatabaseConnection,
    subject_kind: &str,
    subject_id: &str,
) -> Result<Vec<capability_preference::Model>, DbError> {
    validate_subject(subject_kind, subject_id)?;
    capability_preference::Entity::find()
        .filter(capability_preference::Column::SubjectKind.eq(subject_kind))
        .filter(capability_preference::Column::SubjectId.eq(subject_id))
        .all(conn)
        .await
        .map_err(Into::into)
}

pub async fn get_enabled(
    conn: &DatabaseConnection,
    subject_kind: &str,
    subject_id: &str,
    capability: &str,
) -> Result<bool, DbError> {
    validate_key("subject_kind", subject_kind)?;
    validate_key("subject_id", subject_id)?;
    validate_key("capability", capability)?;
    Ok(find_one(conn, subject_kind, subject_id, capability)
        .await?
        .map(|model| model.enabled)
        .unwrap_or(true))
}

pub async fn upsert(
    conn: &DatabaseConnection,
    input: CapabilityPreferenceInput,
) -> Result<capability_preference::Model, DbError> {
    validate_key("subject_kind", &input.subject_kind)?;
    validate_key("subject_id", &input.subject_id)?;
    validate_key("capability", &input.capability)?;
    let now = Utc::now();
    let active = capability_preference::ActiveModel {
        id: NotSet,
        subject_kind: Set(input.subject_kind.clone()),
        subject_id: Set(input.subject_id.clone()),
        capability: Set(input.capability.clone()),
        enabled: Set(input.enabled),
        created_at: Set(now),
        updated_at: Set(now),
    };
    capability_preference::Entity::insert(active)
        .on_conflict(
            OnConflict::columns([
                capability_preference::Column::SubjectKind,
                capability_preference::Column::SubjectId,
                capability_preference::Column::Capability,
            ])
            .update_columns([
                capability_preference::Column::Enabled,
                capability_preference::Column::UpdatedAt,
            ])
            .to_owned(),
        )
        .exec(conn)
        .await?;
    find_one(
        conn,
        &input.subject_kind,
        &input.subject_id,
        &input.capability,
    )
    .await?
    .ok_or_else(|| DbError::Migration("capability preference upsert disappeared".into()))
}

async fn find_one(
    conn: &DatabaseConnection,
    subject_kind: &str,
    subject_id: &str,
    capability: &str,
) -> Result<Option<capability_preference::Model>, DbError> {
    capability_preference::Entity::find()
        .filter(capability_preference::Column::SubjectKind.eq(subject_kind))
        .filter(capability_preference::Column::SubjectId.eq(subject_id))
        .filter(capability_preference::Column::Capability.eq(capability))
        .one(conn)
        .await
        .map_err(Into::into)
}

fn validate_subject(subject_kind: &str, subject_id: &str) -> Result<(), DbError> {
    validate_key("subject_kind", subject_kind)?;
    validate_key("subject_id", subject_id)
}

fn validate_key(name: &str, value: &str) -> Result<(), DbError> {
    if value.trim().is_empty() {
        return Err(DbError::Validation(format!("{name} must not be blank")));
    }
    if value.chars().count() > MAX_KEY_CHARS {
        return Err(DbError::Validation(format!(
            "{name} exceeds {MAX_KEY_CHARS} characters"
        )));
    }
    Ok(())
}
