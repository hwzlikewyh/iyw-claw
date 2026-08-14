use tauri::ipc::{Channel, InvokeResponseBody};

use crate::browser::{
    BrowserError, BrowserFrameSubscriptionSnapshot, BrowserGenerations, BrowserInputEvent,
    BrowserSessionManager,
};

#[tauri::command]
pub async fn browser_subscribe_frames(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    generations: BrowserGenerations,
    on_frame: Channel<InvokeResponseBody>,
) -> Result<BrowserFrameSubscriptionSnapshot, BrowserError> {
    manager
        .subscribe_browser_frames(&tab_id, generations, on_frame)
        .await
}

#[tauri::command]
pub async fn browser_ack_frame(
    manager: tauri::State<'_, BrowserSessionManager>,
    subscription_id: String,
    generations: BrowserGenerations,
    seq: u64,
) -> Result<(), BrowserError> {
    manager
        .acknowledge_browser_frame(&subscription_id, generations, seq)
        .await
}

#[tauri::command]
pub async fn browser_get_frame_subscription(
    manager: tauri::State<'_, BrowserSessionManager>,
    subscription_id: String,
    generations: BrowserGenerations,
) -> Result<BrowserFrameSubscriptionSnapshot, BrowserError> {
    manager
        .browser_frame_subscription(&subscription_id, generations)
        .await
}

#[tauri::command]
pub async fn browser_send_input(
    manager: tauri::State<'_, BrowserSessionManager>,
    subscription_id: String,
    generations: BrowserGenerations,
    events: Vec<BrowserInputEvent>,
) -> Result<(), BrowserError> {
    manager
        .send_browser_input(&subscription_id, generations, events)
        .await
}

#[tauri::command]
pub async fn browser_unsubscribe_frames(
    manager: tauri::State<'_, BrowserSessionManager>,
    subscription_id: String,
    generations: BrowserGenerations,
) -> Result<(), BrowserError> {
    manager
        .unsubscribe_browser_frames(&subscription_id, generations)
        .await
}
