//! 企业微信流式回复由宿主持有 ID、原始 req_id 和结束时限。
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures_util::{stream, StreamExt};
use tokio::sync::Mutex as AsyncMutex;

use super::manager::ChatChannelManager;
use super::task_context::{self, TaskContext};
use super::types::{ChannelMessageTarget, SentMessageId};

const CLOSE_AFTER: Duration = Duration::from_secs(540);
const MIN_REFRESH: Duration = Duration::from_secs(1);
const MAX_CONTENT_BYTES: usize = 20_480;
const MAX_STREAMS: usize = 512;

pub struct StreamUpdate<'a> {
    pub id: &'a str,
    pub content: &'a str,
    pub finished: bool,
}

struct Stream {
    task: TaskContext,
    opened: Instant,
    refreshed: Option<Instant>,
    content: String,
    closed: bool,
}

type StreamHandle = Arc<AsyncMutex<Stream>>;

fn streams() -> &'static Mutex<HashMap<String, StreamHandle>> {
    static STREAMS: OnceLock<Mutex<HashMap<String, StreamHandle>>> = OnceLock::new();
    STREAMS.get_or_init(Mutex::default)
}

fn stream(task: &TaskContext) -> Result<StreamHandle, String> {
    let mut streams = streams().lock().unwrap_or_else(|error| error.into_inner());
    if let Some(stream) = streams.get(&task.id) {
        return Ok(stream.clone());
    }
    if streams.len() >= MAX_STREAMS {
        return Err("CHANNEL_STREAM_CAPACITY".into());
    }
    let stream = Arc::new(AsyncMutex::new(Stream {
        task: task.clone(),
        opened: Instant::now(),
        refreshed: None,
        content: String::new(),
        closed: false,
    }));
    streams.insert(task.id.clone(), stream.clone());
    Ok(stream)
}

pub(crate) async fn update(
    manager: &ChatChannelManager,
    task: &TaskContext,
    update: (&str, bool),
) -> Result<SentMessageId, String> {
    let (content, finished) = update;
    if content.trim().is_empty() || content.len() > MAX_CONTENT_BYTES {
        return Err("INVALID_STREAM_CONTENT".into());
    }
    let handle = stream(task)?;
    let mut stream = handle.lock().await;
    super::task_progress::validate_execution(task).await?;
    if stream.closed || stream.opened.elapsed() >= CLOSE_AFTER {
        return Err("CHANNEL_STREAM_CLOSED".into());
    }
    if !finished
        && stream
            .refreshed
            .is_some_and(|at| at.elapsed() < MIN_REFRESH)
    {
        return Err("CHANNEL_STREAM_RATE_LIMITED".into());
    }
    // 外部发送超时无法确定平台是否接受；预先标记结束，禁止盲目重放。
    stream.closed = finished;
    stream.content = content.to_string();
    stream.refreshed = Some(Instant::now());
    manager
        .send_stream_to_target(
            &task.origin.route.target,
            StreamUpdate {
                id: &task.id,
                content,
                finished,
            },
        )
        .await
        .map_err(|error| error.category().to_string())
}

pub(crate) async fn sweep(manager: &ChatChannelManager) {
    let handles = streams()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .map(|(id, stream)| (id.clone(), stream.clone()))
        .collect::<Vec<_>>();
    stream::iter(handles)
        .for_each_concurrent(Some(4), |entry| close_expired(manager, entry))
        .await;
}

async fn close_expired(manager: &ChatChannelManager, entry: (String, StreamHandle)) {
    let (id, handle) = entry;
    let Ok(mut stream) = handle.try_lock() else {
        return;
    };
    let task = task_context::tracked(&id);
    let execution_ended = super::task_progress::validate_execution(&stream.task)
        .await
        .is_err();
    let should_close = stream.opened.elapsed() >= CLOSE_AFTER || execution_ended;
    if !stream.closed && should_close {
        stream.closed = true;
        let content = if stream.content.is_empty() {
            "本次流式展示已结束，任务状态请查看最新消息。"
        } else {
            &stream.content
        };
        let result = manager
            .send_stream_to_target(
                &stream.task.origin.route.target,
                StreamUpdate {
                    id: &id,
                    content,
                    finished: true,
                },
            )
            .await;
        if let Err(error) = result {
            tracing::warn!(task_id = %id, category = error.category(), "[channel-stream] final refresh failed");
        }
    }
    if task.is_none() {
        streams()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&id);
    }
}

impl ChatChannelManager {
    pub(crate) async fn send_stream_to_target(
        &self,
        target: &ChannelMessageTarget,
        update: StreamUpdate<'_>,
    ) -> Result<SentMessageId, super::error::ChatChannelError> {
        self.backend_for(target.channel_id)
            .await?
            .send_stream_message_to(target, update)
            .await
    }
}
