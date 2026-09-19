use sea_orm::sea_query::{Expr, SimpleExpr};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect, QueryTrait};

use crate::db::entities::{conversation, folder};

pub(super) const ID_BATCH_SIZE: usize = 500;

pub(super) fn non_automation() -> SimpleExpr {
    Expr::cust(
        "NOT EXISTS (SELECT 1 FROM automation_run \
         WHERE automation_run.conversation_id = conversation.id)",
    )
}

pub(super) fn live_folder() -> SimpleExpr {
    conversation::Column::FolderId.in_subquery(
        folder::Entity::find()
            .select_only()
            .column(folder::Column::Id)
            .filter(folder::Column::DeletedAt.is_null())
            .into_query(),
    )
}
