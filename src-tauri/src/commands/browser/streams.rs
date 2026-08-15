use tauri::ipc::{Channel, InvokeResponseBody};

use crate::browser::{
    BrowserFrameSubscriptionSnapshot, BrowserGenerations, BrowserInputEvent, BrowserSessionManager,
};

use super::{browser_command, BrowserCommandFuture};

#[tauri::command(async)]
pub fn browser_subscribe_frames(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    generations: BrowserGenerations,
    on_frame: Channel<InvokeResponseBody>,
) -> BrowserCommandFuture<BrowserFrameSubscriptionSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .subscribe_browser_frames(&tab_id, generations, on_frame)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_ack_frame(
    manager: tauri::State<'_, BrowserSessionManager>,
    subscription_id: String,
    generations: BrowserGenerations,
    seq: u64,
) -> BrowserCommandFuture<()> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .acknowledge_browser_frame(&subscription_id, generations, seq)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_get_frame_subscription(
    manager: tauri::State<'_, BrowserSessionManager>,
    subscription_id: String,
    generations: BrowserGenerations,
) -> BrowserCommandFuture<BrowserFrameSubscriptionSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .browser_frame_subscription(&subscription_id, generations)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_send_input(
    manager: tauri::State<'_, BrowserSessionManager>,
    subscription_id: String,
    generations: BrowserGenerations,
    events: Vec<BrowserInputEvent>,
) -> BrowserCommandFuture<()> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .send_browser_input(&subscription_id, generations, events)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_unsubscribe_frames(
    manager: tauri::State<'_, BrowserSessionManager>,
    subscription_id: String,
    generations: BrowserGenerations,
) -> BrowserCommandFuture<()> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .unsubscribe_browser_frames(&subscription_id, generations)
            .await
    })
}
