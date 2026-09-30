/// 分叉后的重连目标：只信任数据库确认过的会话，避免在未保存完成时猜测重连。
///
/// fork 把活动会话从 S1 切换到 S2，但只有数据库写入成功后才能可靠地重连；
/// 如果运行时在写入之前崩溃，不知道该恢复哪个会话，应该阻止自动重连。
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForkReconnect {
    /// 数据库已确认持久化的分叉会话；崩溃后可以安全重连到这个 ID。
    confirmed: Option<String>,
    /// 标记：分叉已在连接内切换会话，但数据库写入尚未确认。
    unconfirmed: bool,
}

impl ForkReconnect {
    /// 分叉开始切换会话时调用；此时数据库写入尚未完成，崩溃后不应重连。
    pub fn begin_switch(&mut self) {
        self.unconfirmed = true;
    }

    /// 分叉持久化成功后调用，记录新会话 ID 作为重连目标。
    pub fn confirm(&mut self, session_id: String) {
        self.confirmed = Some(session_id);
        self.unconfirmed = false;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconnectTarget {
    /// 可以重连到指定的会话 ID。
    Session(String),
    /// 分叉后活动会话与确认会话不一致，需要用户手动重新打开。
    RebindRequired,
    /// 没有可重连的会话（如全新对话未分叉）。
    Unavailable,
}

/// 根据分叉状态、启动会话和活动会话，决定崩溃后应该重连到哪个会话。
///
/// - 如果分叉尚未确认或活动会话与确认会话不一致，返回 `RebindRequired`。
/// - 如果有确认的分叉会话且与活动会话一致，返回 `Session(确认会话)`。
/// - 否则回退到启动时的会话，或标记为 `Unavailable`。
pub fn reconnect_target(
    fork: &ForkReconnect,
    launch_session_id: Option<&str>,
    live_external_id: Option<&str>,
    recoverable: bool,
) -> ReconnectTarget {
    // 分叉尚未确认持久化：不知道该重连到哪个会话，阻止自动重连。
    if fork.unconfirmed {
        return ReconnectTarget::RebindRequired;
    }

    // 有确认的分叉会话：检查活动会话是否与之一致。
    if let Some(confirmed) = &fork.confirmed {
        return match live_external_id {
            Some(live) if live == confirmed => ReconnectTarget::Session(confirmed.clone()),
            _ => ReconnectTarget::RebindRequired,
        };
    }

    // 没有分叉：回退到启动时的会话（如果有）。
    if let Some(launch) = launch_session_id {
        return ReconnectTarget::Session(launch.to_string());
    }

    // 全新对话分叉后才有会话：需要 agent 支持可恢复会话能力。
    if fork.confirmed.is_some() && recoverable {
        return ReconnectTarget::Session(fork.confirmed.as_ref().unwrap().clone());
    }

    ReconnectTarget::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_fork_uses_launch_session() {
        let fork = ForkReconnect::default();
        assert_eq!(
            reconnect_target(&fork, Some("s1"), None, false),
            ReconnectTarget::Session("s1".to_string())
        );
    }

    #[test]
    fn confirmed_fork_with_matching_live_reconnects_to_new_session() {
        let mut fork = ForkReconnect::default();
        fork.confirm("s2".to_string());
        assert_eq!(
            reconnect_target(&fork, Some("s1"), Some("s2"), false),
            ReconnectTarget::Session("s2".to_string())
        );
    }

    #[test]
    fn unconfirmed_fork_requires_rebind() {
        let mut fork = ForkReconnect::default();
        fork.begin_switch();
        assert_eq!(
            reconnect_target(&fork, Some("s1"), Some("s1"), false),
            ReconnectTarget::RebindRequired
        );
    }

    #[test]
    fn confirmed_fork_with_mismatched_live_requires_rebind() {
        let mut fork = ForkReconnect::default();
        fork.confirm("s2".to_string());
        assert_eq!(
            reconnect_target(&fork, Some("s1"), Some("s3"), false),
            ReconnectTarget::RebindRequired
        );
    }

    #[test]
    fn fresh_conversation_no_fork_unavailable() {
        let fork = ForkReconnect::default();
        assert_eq!(
            reconnect_target(&fork, None, None, false),
            ReconnectTarget::Unavailable
        );
    }

    #[test]
    fn fresh_conversation_with_confirmed_fork_needs_recoverable() {
        let mut fork = ForkReconnect::default();
        fork.confirm("s1".to_string());
        assert_eq!(
            reconnect_target(&fork, None, Some("s1"), false),
            ReconnectTarget::RebindRequired
        );
        assert_eq!(
            reconnect_target(&fork, None, Some("s1"), true),
            ReconnectTarget::Session("s1".to_string())
        );
    }
}
