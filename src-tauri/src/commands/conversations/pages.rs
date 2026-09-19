use crate::app_error::AppCommandError;
use crate::commands::conversation_title::ConversationTitleContext;
use crate::db::service::conversation_service::{self, ConversationPage, ConversationPageRequest};

const SEARCH_SCAN_BUDGET: usize = 1_000;

pub(crate) async fn list_conversations_page_core(
    context: &ConversationTitleContext<'_>,
    mut params: ConversationPageRequest,
) -> Result<ConversationPage, AppCommandError> {
    if params.cursor.is_none() {
        super::refresh_list_titles(context).await;
    }
    let search = params.search.take();
    let limit = params.limit() as usize;
    let mut items = Vec::with_capacity(limit);
    let mut scanned = 0;
    let mut failures = 0;
    loop {
        params.page_size = Some((limit - items.len()).min(SEARCH_SCAN_BUDGET - scanned) as u64);
        let page = conversation_service::list_page(context.conn, &params).await?;
        scanned += page.items.len();
        let (matched, failed) =
            super::search::filter_page(context.conn, page.items, search.as_deref()).await?;
        items.extend(matched);
        failures += failed;
        params.cursor = page.next_cursor;
        if params.cursor.is_none() || items.len() == limit || scanned >= SEARCH_SCAN_BUDGET {
            break;
        }
    }
    Ok(ConversationPage {
        items,
        next_cursor: params.cursor,
        incomplete: failures > 0,
    })
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn list_conversations_page(
    app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::AppDatabase>,
    params: ConversationPageRequest,
) -> Result<ConversationPage, AppCommandError> {
    use tauri::Manager;

    let emitter = crate::web::event_bridge::EventEmitter::Tauri(app.clone());
    let manager = app.state::<crate::chat_channel::manager::ChatChannelManager>();
    let context = ConversationTitleContext {
        conn: &db.conn,
        emitter: &emitter,
        chat_channel_manager: &manager,
    };
    list_conversations_page_core(&context, params).await
}
