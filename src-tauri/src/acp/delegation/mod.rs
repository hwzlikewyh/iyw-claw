//! 内置 HTTP MCP 的异步委派。`delegate_to_agent` 经 listener 和 broker
//! 建立独立 ACP 会话、提交完整任务后返回 task_id，父代理继续执行独立工作。
//! 子会话首次 TurnComplete 将结果写入父会话范围内的缓存并通知界面；父代理
//! 通过 `get_delegation_status` 按需批量收集结果，使用 `cancel_delegation` 停止任务。
//!
//! 任务生命周期、早到结果、取消和连接回收均由现有 broker 管理。
//! MCP 委派为单次会话；星河原生 spawn_agent/followup_task 是另一条协作路径。

mod artifact_listener;
pub mod artifact_tool;
pub mod audio_tool;
pub mod broker;
mod channel_listener;
pub mod companion;
pub mod depth;
pub mod event_emitter;
pub(crate) mod image_format;
pub mod image_loader;
pub mod image_tool;
pub mod listener;
pub mod listener_service;
pub mod live_reply;
pub mod meta_writer;
pub mod mutation_gate;
pub mod parent_watcher;
pub mod spawner;
pub(crate) mod task_prompt;
pub mod transport;
pub mod types;
