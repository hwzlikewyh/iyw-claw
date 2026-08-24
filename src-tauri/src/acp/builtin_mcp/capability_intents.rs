use super::capability_registry::stable_capability_id;

#[derive(Debug, Clone, Copy)]
pub(super) struct CapabilityIntentMetadata {
    pub aliases: &'static [&'static str],
    pub intent_terms: &'static [&'static str],
    pub negative_terms: &'static [&'static str],
    pub when_to_use: &'static str,
}

const EMPTY: &[&str] = &[];

macro_rules! metadata {
    ($aliases:expr, $terms:expr, $when:expr) => {
        CapabilityIntentMetadata {
            aliases: $aliases,
            intent_terms: $terms,
            negative_terms: EMPTY,
            when_to_use: $when,
        }
    };
}

// Explicit phrases cover ambiguous or high-value routes. Other tools inherit
// a small category vocabulary, so every schema entry remains searchable.
#[rustfmt::skip]
const SPECIAL_METADATA: &[(&str, CapabilityIntentMetadata)] = &[
    ("present_task_files", metadata!(&["task artifacts", "任务成果", "提交成果", "交付文件"], &["present", "提交", "artifact", "成果"], "Use to register final files or URLs as task artifacts.")),
    ("get_session_info", metadata!(&["session info", "会话信息", "查看会话"], &["read", "读取", "session", "会话"], "Use to inspect a referenced iyw-claw session.")),
    ("analyze_image", metadata!(&["analyze image", "分析图片", "理解图片"], &["analyze", "分析", "image", "图片"], "Use to inspect image content.")),
    ("append_user_memory", metadata!(&["remember user fact", "记住用户事实", "保存用户偏好", "长期记忆"], &["remember", "记住", "append", "保存", "memory", "记忆"], "Use only for an explicit durable user fact or preference.")),
    ("propose_user_memory", metadata!(&["propose memory", "候选记忆", "记录用户纠正"], &["propose", "候选", "memory", "记忆", "correction", "纠正"], "Use for a conservative reusable correction, preference, or fact.")),
    ("memory_recall", metadata!(&["recall memory", "检索记忆", "查历史记忆"], &["recall", "检索", "memory", "记忆", "history", "历史"], "Use to search stored user memory, never to read account identity.")),
    ("send_channel_messages", metadata!(&["send channel message", "发送消息", "发企业微信消息"], &["send", "发送", "message", "消息"], "Use to send a message through a configured channel.")),
    ("delegate_to_agent", metadata!(&["delegate task", "委派任务", "分派智能体"], &["delegate", "委派", "agent", "智能体"], "Use to delegate a bounded task to another Agent.")),
];

#[rustfmt::skip]
const CATEGORY_METADATA: &[(&str, CapabilityIntentMetadata)] = &[
    ("automation", metadata!(&["scheduled task", "定时任务", "计划任务"], &["task", "任务", "schedule", "定时"], "Use the scheduled-task host capability.")),
    ("browser", metadata!(&["browser", "浏览器", "网页"], &["browser", "浏览器", "page", "页面"], "Use the managed browser host capability.")),
    ("artifacts", metadata!(&["artifact", "成果", "交付"], &["artifact", "成果", "file", "文件"], "Use the task-artifact host capability.")),
    ("delegation", metadata!(&["delegation", "委派", "子任务"], &["delegate", "委派", "task", "任务"], "Use the delegation host capability.")),
    ("interaction", metadata!(&["feedback", "反馈", "询问用户"], &["feedback", "反馈", "question", "问题"], "Use the user-interaction host capability.")),
    ("session", metadata!(&["session", "会话", "用户资料"], &["session", "会话", "profile", "资料"], "Use the session or profile host capability.")),
    ("audio", metadata!(&["audio", "音频", "转写"], &["audio", "音频", "transcribe", "转写"], "Use the audio host capability.")),
    ("image", metadata!(&["image", "图片", "图像"], &["image", "图片", "analyze", "分析"], "Use the image host capability.")),
    ("memory", metadata!(&["memory", "记忆", "记住"], &["memory", "记忆", "remember", "记住"], "Use the memory host capability.")),
    ("channels", metadata!(&["channel", "渠道", "消息"], &["channel", "渠道", "message", "消息"], "Use the message-channel host capability.")),
];

pub(super) fn intent_metadata(tool_name: &str) -> Option<CapabilityIntentMetadata> {
    if let Some((_, metadata)) = SPECIAL_METADATA.iter().find(|(name, _)| *name == tool_name) {
        return Some(*metadata);
    }
    let id = stable_capability_id(tool_name)?;
    let category = id.split('.').nth(1)?;
    CATEGORY_METADATA
        .iter()
        .find_map(|(name, metadata)| (*name == category).then_some(*metadata))
}

pub(super) fn validate_intent_metadata<'a>(
    tool_names: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let missing = tool_names
        .into_iter()
        .filter(|name| intent_metadata(name).is_none())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("intent metadata missing for tools: {missing:?}"))
    }
}
