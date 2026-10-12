use super::types::ChannelRuntimeQuery;
use crate::app_error::AppCommandError;
use crate::chat_channel::manager::ChatChannelManager;
use crate::db::entities::{
    chat_channel_conversation_binding as binding, chat_channel_thread_binding as thread,
    conversation, folder,
};
use crate::db::error::DbError;
use crate::db::service::chat_channel_service;
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use std::collections::HashMap;
use std::path::Path;

pub(super) const PAGE_SIZE: usize = 40;

#[derive(Default)]
pub(super) struct RouteRef {
    pub trace_id: Option<String>,
    pub connection_id: Option<String>,
    pub generation: u64,
}

pub(super) struct ChannelScope {
    pub channel_id: i32,
    pub folders: Vec<i32>,
    pub routes: HashMap<i32, RouteRef>,
}

impl ChannelScope {
    pub(super) async fn load(
        db: &DatabaseConnection,
        manager: &ChatChannelManager,
        channel_id: i32,
    ) -> Result<Self, AppCommandError> {
        let channel = chat_channel_service::get_by_id(db, channel_id)
            .await?
            .ok_or_else(|| AppCommandError::not_found("Message channel not found"))?;
        let config: serde_json::Value = serde_json::from_str(&channel.config_json)
            .map_err(|_| AppCommandError::invalid_input("Invalid message channel configuration"))?;
        let folders = channel_folders(db, &config).await?;
        let mut routes = persisted_routes(db, channel_id).await?;
        let bridge = manager.session_bridge();
        let guard = bridge.lock().await;
        for session in guard
            .all_sessions()
            .filter(|session| session.channel_id == channel_id)
        {
            let route = routes.entry(session.conversation_id).or_default();
            if session.registration_generation >= route.generation {
                *route = RouteRef {
                    trace_id: session.trace_id.clone(),
                    connection_id: Some(session.connection_id.clone()),
                    generation: session.registration_generation,
                };
            }
        }
        Ok(Self {
            channel_id,
            folders,
            routes,
        })
    }

    fn condition(&self) -> Condition {
        Condition::any()
            .add(conversation::Column::FolderId.is_in(self.folders.clone()))
            .add(conversation::Column::Id.is_in(self.routes.keys().copied().collect::<Vec<_>>()))
    }

    pub(super) async fn page(
        &self,
        db: &DatabaseConnection,
        query: &ChannelRuntimeQuery,
    ) -> Result<Vec<conversation::Model>, AppCommandError> {
        let mut select = conversation::Entity::find()
            .filter(self.condition())
            .filter(conversation::Column::DeletedAt.is_null())
            .filter(conversation::Column::ParentId.is_null())
            .filter(conversation::Column::Kind.ne(conversation::ConversationKind::Loop));
        if let Some(before) = query.before.as_ref() {
            select = select.filter(
                Condition::any()
                    .add(conversation::Column::UpdatedAt.lt(before.updated_at))
                    .add(
                        Condition::all()
                            .add(conversation::Column::UpdatedAt.eq(before.updated_at))
                            .add(conversation::Column::Id.lt(before.id)),
                    ),
            );
        }
        if let Some(search) = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            select = select.filter(conversation::Column::Title.contains(search));
        }
        Ok(select
            .order_by_desc(conversation::Column::UpdatedAt)
            .order_by_desc(conversation::Column::Id)
            .limit((PAGE_SIZE + 1) as u64)
            .all(db)
            .await
            .map_err(DbError::from)?)
    }

    pub(super) async fn conversation(
        &self,
        db: &DatabaseConnection,
        id: i32,
    ) -> Result<conversation::Model, AppCommandError> {
        conversation::Entity::find_by_id(id)
            .filter(self.condition())
            .filter(conversation::Column::DeletedAt.is_null())
            .one(db)
            .await
            .map_err(DbError::from)?
            .ok_or_else(|| {
                AppCommandError::not_found("Conversation is not associated with this channel")
            })
    }
}

async fn channel_folders(
    db: &DatabaseConnection,
    config: &serde_json::Value,
) -> Result<Vec<i32>, AppCommandError> {
    let root = config
        .get("channel_workspace_root")
        .and_then(|value| value.as_str())
        .map(crate::parsers::normalize_path_for_matching)
        .filter(|root| !root.is_empty());
    let legacy = config
        .get("default_folder_id")
        .and_then(|value| value.as_i64());
    let rows = folder::Entity::find()
        .filter(folder::Column::DeletedAt.is_null())
        .all(db)
        .await
        .map_err(DbError::from)?;
    Ok(rows
        .into_iter()
        .filter(|row| {
            legacy == Some(i64::from(row.id))
                || root.as_deref().is_some_and(|root| {
                    Path::new(&crate::parsers::normalize_path_for_matching(&row.path))
                        .starts_with(root)
                })
        })
        .map(|row| row.id)
        .collect())
}

async fn persisted_routes(
    db: &DatabaseConnection,
    channel_id: i32,
) -> Result<HashMap<i32, RouteRef>, AppCommandError> {
    let mut routes = HashMap::new();
    for row in binding::Entity::find()
        .filter(binding::Column::ChannelId.eq(channel_id))
        .all(db)
        .await
        .map_err(DbError::from)?
    {
        routes.insert(row.conversation_id, RouteRef::default());
    }
    for row in thread::Entity::find()
        .filter(thread::Column::ChannelId.eq(channel_id))
        .all(db)
        .await
        .map_err(DbError::from)?
    {
        let route = routes.entry(row.conversation_id).or_default();
        if route.connection_id.is_none() {
            route.connection_id = row.connection_id;
        }
    }
    Ok(routes)
}
